#!/usr/bin/env python3
"""Local stdio MCP adapter for the occt-model subprocess and authoring schemas."""
import argparse
import asyncio
import base64
import contextlib
import copy
import hashlib
import json
import math
import pathlib
import re
import shutil
import subprocess
import sys
import uuid

PROTOCOLS = ('2025-11-25', '2025-06-18')
SCHEMAS = ('request', 'model', 'feature', 'parameter', 'sketch', 'requirement', 'inspection', 'face_selector', 'edge_selector', 'edit', 'change', 'view')
EXAMPLES = ('bracket', 'enclosure', 'shaft', 'mating-parts', 'sketch-block', 'sketch-conflict', 'sketch-advanced', 'extrusion-limits', 'curved-extrusions', 'drill-point', 'hole-limits', 'profile-loft', 'curved-pipe', 'revolved-ring', 'symmetric-revolve', 'hollow-profile', 'hollow-sweep', 'hollow-loft', 'mirrored-part', 'scaled-part', 'spring', 'offset-part', 'multi-hole-plate', 'patterned-plate', 'bolt-circle', 'threaded-rod', 'edge-treatments', 'variable-fillet', 'equal-radius-plate', 'face-pocket', 'projected-pocket', 'concentric-bushing', 'tangent-boss', 'arc-tangent-boss')
MAX_MESSAGE = 64 * 1024 * 1024
MAX_RESOURCE = 32 * 1024 * 1024
ROOT = pathlib.Path(__file__).resolve().parents[2]

class RpcError(Exception):
    def __init__(self, code, message):
        self.code, self.message = code, message
        super().__init__(message)

def encode(value):
    return json.dumps(value, separators=(',', ':'), allow_nan=False)

def reject_constant(value):
    raise ValueError(f'nonfinite JSON number: {value}')

def decode(value):
    return json.loads(value, parse_constant=reject_constant)

def tool_result(value, error=False):
    return {'content': [{'type': 'text', 'text': encode(value)}],
            'structuredContent': value, 'isError': error}

def object_schema(properties, required):
    return dict(type='object', properties=properties, required=required, additionalProperties=False)

class Server:
    def __init__(self, binary, output_root, timeout=120):
        self.binary = pathlib.Path(binary).resolve(strict=True)
        self.output_root = pathlib.Path(output_root).resolve()
        self.output_root.mkdir(parents=True, exist_ok=True)
        self.timeout = timeout
        self.state = 'new'
        self.pending = {}
        self.schemas = {}
        for name in SCHEMAS:
            process = subprocess.run([str(self.binary), '--schema', name], capture_output=True, text=True, timeout=30)
            if process.returncode:
                raise RuntimeError(f'schema startup failed: {process.stderr}')
            self.schemas[name] = decode(process.stdout)
        self.examples = {name: decode((ROOT / 'tools/model' / (name + '.request.json')).read_text()) for name in EXAMPLES}
        self.build_inspection_schema = copy.deepcopy(self.schemas['inspection'])
        for name in ('schema', 'model'):
            self.build_inspection_schema['properties'].pop(name)
        self.build_inspection_schema['properties']['build_id'] = dict(type='string', pattern='^[0-9a-f]{32}$')
        self.build_inspection_schema['required'] = ['build_id']
        self.build_edit_schema = copy.deepcopy(self.schemas['edit'])
        for name in ('schema', 'model', 'source'):
            self.build_edit_schema['properties'].pop(name)
        self.build_edit_schema['properties']['build_id'] = dict(type='string', pattern='^[0-9a-f]{32}$')
        self.build_edit_schema['properties']['expected_model_sha256'] = dict(type='string', pattern='^[0-9a-f]{64}$')
        self.build_edit_schema['required'] = [name for name in self.build_edit_schema['required'] if name not in ('schema', 'model')]
        self.build_edit_schema['required'].extend(['build_id', 'expected_model_sha256'])

    def tools(self):
        read = dict(readOnlyHint=True, destructiveHint=False, idempotentHint=True, openWorldHint=False)
        return [
            dict(name='occt_get_schema', description='Read the complete generated JSON Schema for an engine authoring type. Semantic/unit/geometry checks run at build time.',
                 inputSchema=object_schema({'name': dict(type='string', enum=list(SCHEMAS))}, ['name']), annotations=read),
            dict(name='occt_get_example', description='Read a complete editable example request; dimensions are demonstration assumptions.',
                 inputSchema=object_schema({'name': dict(type='string', enum=list(EXAMPLES))}, ['name']), annotations=read),
            dict(name='occt_inspect_model', description='Read paged model/instance parameters, feature inputs, datums, references and requirements. Optional output regeneration measures faces/edges and tests semantic selectors. Does not publish or edit a build.',
                 inputSchema=self.schemas['inspection'], annotations=read),
            dict(name='occt_inspect_build', description='Inspect an accepted build by ID without resending its model. Use instance/output and optional semantic selectors for geometry queries; snapshot indices are not persistent identities.',
                 inputSchema=self.build_inspection_schema, annotations=read),
            dict(name='occt_visualize_model', description='Create a self-contained interactive annotated viewer for sketches and 3D shapes. Dimensions, driving values, solver constraint residuals, and failed requirement witnesses are linked to geometry. Diagnostic snapshots never become accepted builds.',
                 inputSchema=self.schemas['view'], annotations=dict(readOnlyHint=True, destructiveHint=False, idempotentHint=False, openWorldHint=False)),
            dict(name='occt_edit_build', description='Apply guarded edits to an accepted model snapshot: add/replace/remove features, add parameters/requirements/references, and set parameter overrides. Requires expected model SHA-256 and explicit revision metadata. Preserves existing requirements and source build; verifies and publishes a new build with a semantic change record.',
                 inputSchema=self.build_edit_schema, annotations=dict(readOnlyHint=False, destructiveHint=False, idempotentHint=False, openWorldHint=False)),
            dict(name='occt_build', description='Validate/migrate a model, apply typed parameter edits, verify requirements, and build selected parts into a new server-owned directory. Returns report and artifact resource URIs. Failed edits preserve previous builds.',
                 inputSchema=self.schemas['request'], annotations=dict(readOnlyHint=False, destructiveHint=False, idempotentHint=False, openWorldHint=False)),
        ]

    def fixed_resources(self):
        return [dict(uri=f'occt://schema/{name}', name=f'{name} schema', mimeType='application/schema+json') for name in SCHEMAS] + [
            dict(uri=f'occt://example/{name}', name=f'{name} example request', mimeType='application/json') for name in EXAMPLES]

    @staticmethod
    def named(arguments, choices):
        if not isinstance(arguments, dict) or set(arguments) != {'name'} or arguments['name'] not in choices:
            raise RpcError(-32602, 'expected a supported name and no other arguments')
        return arguments['name']

    def resource(self, uri):
        if not isinstance(uri, str):
            raise RpcError(-32602, 'resource URI must be a string')
        for prefix, values, mime in [('occt://schema/', self.schemas, 'application/schema+json'), ('occt://example/', self.examples, 'application/json')]:
            if uri.startswith(prefix) and uri[len(prefix):] in values:
                return {'contents': [dict(uri=uri, mimeType=mime, text=encode(values[uri[len(prefix):]]))]}
        match = re.fullmatch(r'occt://build/([0-9a-f]{32})/([a-zA-Z0-9_.-]+)', uri)
        if not match:
            raise RpcError(-32002, 'unknown resource')
        build_id, artifact = match.groups()
        directory = self.output_root / build_id / 'build'
        report_path = directory / 'report.json'
        if not directory.resolve().is_relative_to(self.output_root) or not report_path.is_file() or report_path.is_symlink():
            raise RpcError(-32002, 'build is incomplete or unknown')
        report = decode(self.read_file(report_path))
        allowed = {'report.json', *report['artifacts'].values()}
        for output in report['outputs']:
            allowed.update(output[key] for key in ('preview', 'stl') if key in output)
        path = directory / artifact
        if artifact not in allowed or path.is_symlink() or not path.resolve().is_relative_to(self.output_root):
            raise RpcError(-32002, 'unknown build artifact')
        data = self.read_file(path)
        mime = {'.json': 'application/json', '.svg': 'image/svg+xml', '.html': 'text/html', '.stl': 'model/stl', '.step': 'application/step'}.get(path.suffix, 'application/octet-stream')
        content = dict(uri=uri, mimeType=mime)
        if path.suffix in ('.json', '.svg', '.html'):
            content['text'] = data.decode('utf-8')
        else:
            content['blob'] = base64.b64encode(data).decode('ascii')
        return {'contents': [content]}

    @staticmethod
    def read_file(path):
        with path.open('rb') as file:
            data = file.read(MAX_RESOURCE + 1)
        if len(data) > MAX_RESOURCE:
            raise RpcError(-32002, 'resource exceeds the 32 MiB read limit; use the local artifact path')
        return data

    async def build(self, request):
        return await self.worker(request)

    async def inspect_model(self, request):
        return await self.worker(request, inspection=True)

    def accepted_model(self, build_id):
        report = decode(self.resource(f"occt://build/{build_id}/report.json")['contents'][0]['text'])
        if report.get('status') != 'built':
            raise RpcError(-32602, 'diagnostic visualization is not an accepted model build')
        return self.resource(f"occt://build/{build_id}/model.json")['contents'][0]['text']

    async def inspect_build(self, arguments):
        allowed = set(self.build_inspection_schema['properties'])
        if not isinstance(arguments, dict) or not set(arguments).issubset(allowed) or not isinstance(arguments.get('build_id'), str) or not re.fullmatch(r'[0-9a-f]{32}', arguments['build_id']):
            raise RpcError(-32602, 'expected a build_id and supported inspection options')
        text = self.accepted_model(arguments['build_id'])
        request = {key: value for key, value in arguments.items() if key != 'build_id'}
        request.update(schema='occb-model-inspection-v1', model=decode(text))
        result = await self.inspect_model(request)
        if not result['isError']:
            value = result['structuredContent']
            value['source'] = dict(build_id=arguments['build_id'], model_sha256=hashlib.sha256(text.encode('utf-8')).hexdigest())
            return tool_result(value)
        return result

    async def edit_build(self, arguments):
        allowed = set(self.build_edit_schema['properties'])
        if not isinstance(arguments, dict) or not set(arguments).issubset(allowed) or not isinstance(arguments.get('build_id'), str) or not re.fullmatch(r'[0-9a-f]{32}', arguments['build_id']) or not isinstance(arguments.get('expected_model_sha256'), str) or not re.fullmatch(r'[0-9a-f]{64}', arguments['expected_model_sha256']):
            raise RpcError(-32602, 'expected build_id, expected_model_sha256, and supported edit fields')
        text = self.accepted_model(arguments['build_id'])
        fingerprint = hashlib.sha256(text.encode('utf-8')).hexdigest()
        if fingerprint != arguments['expected_model_sha256']:
            return tool_result(dict(status='failed', stage='conflict', message='source model fingerprint differs from the inspected snapshot'), True)
        request = {key: value for key, value in arguments.items() if key not in ('build_id', 'expected_model_sha256')}
        request.update(schema='occb-model-edit-v1', model=decode(text), source=dict(build_id=arguments['build_id'], model_sha256=fingerprint))
        return await self.worker(request, editing=True)

    async def worker(self, request, inspection=False, editing=False, visualization=False):
        if not isinstance(request, dict):
            raise RpcError(-32602, 'build arguments must be a model request object')
        # Paths and executable are configured by the local operator; tool callers
        # only provide the model request. No caller strings become filesystem names.
        build_id = uuid.uuid4().hex
        work = self.output_root / build_id
        work.mkdir(mode=0o700)
        process = None
        accepted = False
        try:
            (work / 'request.json').write_text(encode(request))
            report_path = work / 'inspection.json' if inspection else work / 'build/report.json'
            command = [str(self.binary)]
            if inspection:
                command.append('--inspect')
            elif editing:
                command.append('--edit')
            elif visualization:
                command.append('--visualize')
            command.extend([str(work / 'request.json'), str(report_path if inspection else work / 'build')])
            with (work / 'kernel.stderr').open('wb') as stderr:
                process = await asyncio.create_subprocess_exec(*command,
                    stdin=asyncio.subprocess.DEVNULL, stdout=asyncio.subprocess.DEVNULL, stderr=stderr)
                await asyncio.wait_for(process.wait(), self.timeout)
            if process.returncode != 0:
                with (work / 'kernel.stderr').open('rb') as file:
                    file.seek(0, 2)
                    length = file.tell()
                    file.seek(max(0, length - 1024 * 1024))
                    lines = file.read().decode('utf-8', errors='replace').splitlines()
                try:
                    error = decode(lines[-1])
                except (ValueError, IndexError):
                    error = dict(status='failed', stage='kernel', message='worker failed without a structured report', diagnostics=[])
                return tool_result(error, True)
            report = decode(self.read_file(report_path))
            if inspection:
                return tool_result(report)
            filenames = {'report.json', *report['artifacts'].values()}
            for output in report['outputs']:
                filenames.update(output[key] for key in ('preview', 'stl') if key in output)
            with (work / 'build/model.json').open('rb') as file:
                fingerprint = hashlib.file_digest(file, 'sha256').hexdigest()
            accepted = True
            result = dict(model_sha256=fingerprint, report=report, directory=str(work / 'build'),resources={name: f'occt://build/{build_id}/{name}' for name in sorted(filenames)})
            result['visualization_id' if visualization else 'build_id'] = build_id
            return tool_result(result)
        except TimeoutError:
            return tool_result(dict(status='failed', stage='timeout', message=f'build exceeded {self.timeout:g} seconds'), True)
        finally:
            if process is not None and process.returncode is None:
                with contextlib.suppress(ProcessLookupError):
                    process.terminate()
                try:
                    await asyncio.wait_for(process.wait(), 5)
                except TimeoutError:
                    with contextlib.suppress(ProcessLookupError):
                        process.kill()
                    await process.wait()
            if not accepted:
                shutil.rmtree(work)

    async def dispatch(self, message):
        method = message['method']
        params = message.get('params', {})
        if not isinstance(params, dict):
            raise RpcError(-32602, 'params must be an object')
        if method == 'ping':
            return {}
        if method == 'initialize':
            if self.state != 'new':
                raise RpcError(-32600, 'already initialized')
            if not isinstance(params.get('protocolVersion'), str) or not isinstance(params.get('capabilities'), dict) or not isinstance(params.get('clientInfo'), dict):
                raise RpcError(-32602, 'initialize requires protocolVersion, capabilities, and clientInfo')
            if not all(isinstance(params['clientInfo'].get(key), str) for key in ('name', 'version')):
                raise RpcError(-32602, 'clientInfo requires string name and version')
            self.state = 'initializing'
            return dict(protocolVersion=params['protocolVersion'] if params['protocolVersion'] in PROTOCOLS else PROTOCOLS[0],
                        capabilities=dict(tools=dict(listChanged=False), resources=dict(listChanged=False)),
                        serverInfo=dict(name='occt-model', version='0.1.0'),
                        instructions='Read a schema/example, preserve requirements and explicit assumptions, then build. Read model.json to continue from the accepted model. Preview SVGs are sampled; inspect requirement evidence.')
        if self.state != 'ready':
            raise RpcError(-32600, 'initialize and send notifications/initialized first')
        if method == 'tools/list':
            if params.get('cursor'):
                raise RpcError(-32602, 'no pagination cursor is supported')
            return dict(tools=self.tools())
        if method == 'tools/call':
            name, arguments = params.get('name'), params.get('arguments', {})
            if name == 'occt_get_schema':
                return tool_result(self.schemas[self.named(arguments, SCHEMAS)])
            if name == 'occt_get_example':
                return tool_result(self.examples[self.named(arguments, EXAMPLES)])
            if name == 'occt_inspect_model':
                return await self.inspect_model(arguments)
            if name == 'occt_inspect_build':
                return await self.inspect_build(arguments)
            if name == 'occt_visualize_model':
                return await self.worker(arguments, visualization=True)
            if name == 'occt_edit_build':
                return await self.edit_build(arguments)
            if name == 'occt_build':
                return await self.build(arguments)
            raise RpcError(-32602, 'unknown tool')
        if method == 'resources/list':
            return dict(resources=self.fixed_resources())
        if method == 'resources/templates/list':
            return dict(resourceTemplates=[dict(uriTemplate='occt://build/{build_id}/{artifact}', name='Accepted build artifact',
                description='Use artifact URIs returned by occt_build: model/report JSON, SVG previews, binary STL or STEP; maximum read size 32 MiB.')])
        if method == 'resources/read':
            return self.resource(params.get('uri'))
        raise RpcError(-32601, 'method not found')

    @staticmethod
    def send(value):
        sys.stdout.write(encode(value) + '\n')
        sys.stdout.flush()

    async def respond(self, message):
        identity = message['id']
        try:
            result = await self.dispatch(message)
            self.send(dict(jsonrpc='2.0', id=identity, result=result))
        except RpcError as error:
            self.send(dict(jsonrpc='2.0', id=identity, error=dict(code=error.code, message=error.message)))
        except asyncio.CancelledError:
            # Cancellation does not require a response; cleanup is awaited by build.
            pass
        except Exception as error:
            print(f'occt-model MCP: {type(error).__name__}: {error}', file=sys.stderr)
            self.send(dict(jsonrpc='2.0', id=identity, error=dict(code=-32603, message='internal server error')))
        finally:
            self.pending.pop(identity, None)

    async def serve(self):
        reader = asyncio.StreamReader(limit=MAX_MESSAGE)
        transport, _ = await asyncio.get_running_loop().connect_read_pipe(
            lambda: asyncio.StreamReaderProtocol(reader), sys.stdin.buffer)
        try:
            while True:
                try:
                    line = await reader.readline()
                except ValueError:
                    self.send(dict(jsonrpc='2.0', id=None, error=dict(code=-32600, message='message exceeds 64 MiB')))
                    break
                if not line:
                    break
                try:
                    message = decode(line)
                except (ValueError, UnicodeDecodeError):
                    self.send(dict(jsonrpc='2.0', id=None, error=dict(code=-32700, message='parse error')))
                    continue
                valid_id = isinstance(message, dict) and ('id' not in message or (isinstance(message['id'], (str, int)) and not isinstance(message['id'], bool)))
                if not valid_id or message.get('jsonrpc') != '2.0' or not isinstance(message.get('method'), str):
                    self.send(dict(jsonrpc='2.0', id=None, error=dict(code=-32600, message='invalid request')))
                    continue
                if 'id' not in message:
                    params = message.get('params', {})
                    if message['method'] == 'notifications/initialized' and self.state == 'initializing':
                        self.state = 'ready'
                    elif message['method'] == 'notifications/cancelled' and isinstance(params, dict):
                        identity = params.get('requestId')
                        if isinstance(identity, (str, int)) and not isinstance(identity, bool):
                            task = self.pending.get(identity)
                            if task and not task.cancelling():
                                task.cancel()
                    continue
                if message['id'] in self.pending:
                    self.send(dict(jsonrpc='2.0', id=message['id'], error=dict(code=-32600, message='request ID is already pending')))
                elif len(self.pending) >= 8:
                    self.send(dict(jsonrpc='2.0', id=message['id'], error=dict(code=-32000, message='at most eight pending requests are supported')))
                else:
                    self.pending[message['id']] = asyncio.create_task(self.respond(message))
        finally:
            transport.close()
            tasks = list(self.pending.values())
            for task in tasks:
                if not task.cancelling():
                    task.cancel()
            if tasks:
                await asyncio.gather(*tasks, return_exceptions=True)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, help='absolute path to built occt-model executable')
    parser.add_argument('--output-root', required=True, help='directory for persistent server-owned build artifacts')
    parser.add_argument('--timeout', type=float, default=120, help='per-build timeout in seconds (default 120)')
    args = parser.parse_args()
    if not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error('--timeout must be finite and positive')
    asyncio.run(Server(args.binary, args.output_root, args.timeout).serve())

if __name__ == '__main__':
    main()
