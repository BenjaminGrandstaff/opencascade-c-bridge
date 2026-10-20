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
import os
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
import uuid

PROTOCOLS = ('2025-11-25', '2025-06-18')
SCHEMAS = ('request', 'model', 'feature', 'parameter', 'sketch', 'requirement', 'inspection', 'face_selector', 'edge_selector', 'edit', 'change', 'sketch_change', 'view')
EXAMPLES = tuple(sorted(path.name.removesuffix('.request.json') for path in pathlib.Path(__file__).resolve().parent.glob('*.request.json')))
# Tool name -> executable suffix, report filename, output is a single file.
COMMANDS = {
    'occt_export_drawings': ('drawing-export', 'manifest.json', False),
    'occt_inspection_report': ('inspection-report', 'report.json', True),
    'occt_motion_study': ('motion-study', 'motion.report.json', False),
    'occt_balance_report': ('balance-report', 'report.json', True),
    'occt_joint_branches': ('joint-branches', 'report.json', False),
    'occt_merge_documents': ('document-merge', 'report.json', False),
}
COMMAND_DESCRIPTIONS = {
    'occt_export_drawings': 'Export saved or supplied drawing sheets to SVG and millimeter DXF. Returns a retained drawing model and manifest. Does not modify or accept a model build.',
    'occt_inspection_report': 'Evaluate supplied dimensions, positions, measured points, surface textures and thread diameters against saved drawings. Nonconforming results retain the complete report with isError true.',
    'occt_motion_study': 'Run sampled and continuous collision checks for configured hinge or slider components. Retains assembly, study and report even when collision checks fail or remain unresolved.',
    'occt_balance_report': 'Measure selected components using explicit materials/densities and an inline reference chord. Returns mass, inertia and geometric center of gravity; no recommended CG range is assumed.',
    'occt_joint_branches': 'Search alternative linkage poses from explicit seeds and return the search status plus reloadable branch models. A bounded search does not prove all branches were found.',
    'occt_merge_documents': 'Three-way semantic merge of base, current and other model snapshots. Returns a merged model or complete conflict report. Inputs and accepted builds remain unchanged; build a merged model separately to verify geometry and requirements.',
}
MAX_MESSAGE = 64 * 1024 * 1024
DEFAULT_STUDIO = 'http://127.0.0.1:37391'

MAX_RESOURCE = 32 * 1024 * 1024
# Renders attached to a build result as image content; the rest stay resources.
MAX_INLINE_IMAGES = 8
MAX_INLINE_BYTES = 4 * 1024 * 1024
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

def tool_result(value, error=False, images=()):
    content = [{'type': 'text', 'text': encode(value)}]
    content.extend({'type': 'image', 'mimeType': 'image/png', 'data': base64.b64encode(data).decode('ascii')} for data in images)
    return {'content': content, 'structuredContent': value, 'isError': error}

def studio_base(url):
    """The base URL of a local studio (occt-view --serve); other hosts are refused."""
    try:
        parts = urllib.parse.urlsplit(url) if isinstance(url, str) else None
        port = parts.port if parts else None
    except ValueError:
        parts = port = None
    if not parts or parts.scheme != 'http' or parts.hostname not in ('127.0.0.1', 'localhost') or port is None \
            or parts.path not in ('', '/') or parts.query or parts.fragment or parts.username or parts.password:
        raise RpcError(-32602, 'studio url must be http://127.0.0.1:PORT or http://localhost:PORT')
    return f'http://{parts.hostname}:{port}'

def output_files(report):
    """Per-output artifact filenames a build report lists."""
    files = set()
    for output in report['outputs']:
        files.update(output[key] for key in ('preview', 'stl') if key in output)
        files.update(image['file'] for image in output.get('renders', ()) if 'file' in image)
    return files

def object_schema(properties, required):
    return dict(type='object', properties=properties, required=required, additionalProperties=False)

class Server:
    def __init__(self, binary, output_root, timeout=120, commands_dir=None, studio_url=None):
        self.studio = studio_base(studio_url or os.environ.get('OCCT_STUDIO_URL') or DEFAULT_STUDIO)
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
        command_directory = pathlib.Path(commands_dir).resolve() if commands_dir else self.binary.parent
        self.commands = {}
        for tool, (suffix, _, _) in COMMANDS.items():
            executable = command_directory / ('occt-' + suffix)
            if not executable.is_file():
                continue
            process = subprocess.run([str(executable), '--schema'], capture_output=True, text=True, timeout=30)
            if process.returncode:
                raise RuntimeError(f'{suffix} schema startup failed: {process.stderr}')
            generated = decode(process.stdout)
            version = generated['$defs']['ModelDocument']['properties']['schema_version']['const']
            if version != self.schemas['model']['properties']['schema_version']['const']:
                raise RuntimeError(f'{suffix} model schema differs from occt-model; rebuild the commands together')
            self.schemas[suffix.replace('-', '_')] = generated
            self.commands[tool] = executable
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

        self.catalog = {
            'schema_version': self.schemas['model']['properties']['schema_version']['const'],
            'schemas': list(self.schemas),
            'unavailable_tools': sorted(set(COMMANDS) - set(self.commands)),
            'feature_operations': sorted(next(iter(v['properties'])) for v in self.schemas['request']['$defs']['FeatureOperation']['oneOf']),
            'examples': {name: request['schema'] for name, request in self.examples.items()},
            'counts': {'tools': len(self.tools()), 'schemas': len(self.schemas), 'examples': len(self.examples),
                       'resources': len(self.fixed_resources()),
                       'feature_operations': len(self.schemas['request']['$defs']['FeatureOperation']['oneOf']),
                       'request_definitions': len(self.schemas['request']['$defs'])},
        }

    def tools(self):
        read = dict(readOnlyHint=True, destructiveHint=False, idempotentHint=True, openWorldHint=False)
        return [
            dict(name='occt_get_schema', description='Read the complete generated JSON Schema for an engine authoring type. Semantic/unit/geometry checks run at build time.',
                 inputSchema=object_schema({'name': dict(type='string', enum=list(self.schemas))}, ['name']), annotations=read),
            dict(name='occt_get_example', description='Read a complete editable example request; dimensions are demonstration assumptions.',
                 inputSchema=object_schema({'name': dict(type='string', enum=list(EXAMPLES))}, ['name']), annotations=read),
            dict(name='occt_inspect_model', description='Read paged model/instance parameters, feature inputs, datums, references and requirements. Use sketch_feature with an instance for isolated sketch solver/geometry diagnosis and guarded repair candidates. Use loft_feature with an instance for isolated exact boundary, partition, start, ordered edge-count and hollow-station material-region diagnostics, even when the loft itself fails. Optional output regeneration measures faces/edges and tests semantic selectors. Does not publish or edit a build.',
                 inputSchema=self.schemas['inspection'], annotations=read),
            dict(name='occt_inspect_build', description='Inspect an accepted build by ID without resending its model. Use instance/output and optional semantic selectors for geometry queries; snapshot indices are not persistent identities.',
                 inputSchema=self.build_inspection_schema, annotations=read),
            dict(name='occt_visualize_model', description='Create a self-contained interactive annotated viewer for sketches and 3D shapes. Dimensions, driving values, solver constraint residuals, and failed requirement witnesses are linked to geometry. Diagnostic snapshots never become accepted builds.',
                 inputSchema=self.schemas['view'], annotations=dict(readOnlyHint=True, destructiveHint=False, idempotentHint=False, openWorldHint=False)),
            dict(name='occt_preview_edit_build', description='Preview guarded edits against an accepted build snapshot. Returns diagnostic candidate geometry and solver reports without accepting the candidate, recording a revision, or modifying the source build. Repair suggestions require a successful verified edit before publication. Set renders to see the candidate as shaded PNG views and sections.',
                 inputSchema=self.build_edit_schema, annotations=dict(readOnlyHint=True, destructiveHint=False, idempotentHint=False, openWorldHint=False)),
            dict(name='occt_edit_build', description='Apply guarded feature, sketch element/constraint/configuration and named-reference edits to an accepted model snapshot, or set parameter overrides. Requires expected model SHA-256 and explicit revision metadata. Preserves existing requirements and source build; verifies and publishes a new build with a semantic change record.',
                 inputSchema=self.build_edit_schema, annotations=dict(readOnlyHint=False, destructiveHint=False, idempotentHint=False, openWorldHint=False)),
            dict(name='occt_build', description='Validate/migrate a model, apply typed parameter edits, verify requirements, and build selected parts into a new server-owned directory. Returns report and artifact resource URIs. Failed edits preserve previous builds. Set renders (for example {} or views plus section planes) to get shaded PNG views of each output; up to eight come back as image content and all stay readable as resources.',
                 inputSchema=self.schemas['request'], annotations=dict(readOnlyHint=False, destructiveHint=False, idempotentHint=False, openWorldHint=False)),
            dict(name='occt_session_read', description='Read the model open in a local studio (occt-view --serve) with its model_sha256, version, unsaved state and last shared change. Build or edit that model to verify a change, then apply the accepted build with occt_session_apply.',
                 inputSchema=object_schema({'url': dict(type='string', description=f'Studio URL; default {self.studio}')}, []), annotations=read),
            dict(name='occt_session_apply', description='Apply an accepted build to the local studio session while the studio model still has expected_model_sha256 (from occt_session_read). A person editing meanwhile makes this fail with stage conflict: read again and rebase. The change joins the studio undo history and stays unsaved until the person saves.',
                 inputSchema=object_schema({'build_id': dict(type='string', pattern='^[0-9a-f]{32}$'), 'expected_model_sha256': dict(type='string', pattern='^[0-9a-f]{64}$'),
                     'message': dict(type='string', minLength=1, maxLength=200), 'author': dict(type='string', minLength=1, maxLength=200), 'url': dict(type='string')},
                     ['build_id', 'expected_model_sha256', 'message']),
                 annotations=dict(readOnlyHint=False, destructiveHint=False, idempotentHint=False, openWorldHint=False)),
        ] + [dict(name=name, description=COMMAND_DESCRIPTIONS[name],
                  inputSchema=self.schemas[COMMANDS[name][0].replace('-', '_')],
                  annotations=dict(readOnlyHint=True, destructiveHint=False, idempotentHint=False, openWorldHint=False))
             for name in self.commands]

    def fixed_resources(self):
        return [dict(uri='occt://catalog', name='Current authoring capabilities and examples', mimeType='application/json')] + [dict(uri=f'occt://schema/{name}', name=f'{name} schema', mimeType='application/schema+json') for name in self.schemas] + [
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
        if uri == 'occt://catalog':
            return {'contents': [dict(uri=uri, mimeType='application/json', text=encode(self.catalog))]}
        analysis = re.fullmatch(r'occt://analysis/([0-9a-f]{32})/([a-zA-Z0-9_.-]+)', uri)
        if analysis:
            identity, artifact = analysis.groups()
            work = self.output_root / identity
            manifest = work / 'analysis-artifacts.json'
            if not manifest.is_file() or manifest.is_symlink():
                raise RpcError(-32002, 'analysis is incomplete or unknown')
            allowed = decode(self.read_file(manifest))
            path = work / 'results' / artifact
            if artifact not in allowed or path.is_symlink() or not path.resolve().is_relative_to(work / 'results'):
                raise RpcError(-32002, 'unknown analysis artifact')
            return self.artifact_content(uri, path)
        match = re.fullmatch(r'occt://build/([0-9a-f]{32})/([a-zA-Z0-9_.-]+)', uri)
        if not match:
            raise RpcError(-32002, 'unknown resource')
        build_id, artifact = match.groups()
        directory = self.output_root / build_id / 'build'
        report_path = directory / 'report.json'
        if not directory.resolve().is_relative_to(self.output_root) or not report_path.is_file() or report_path.is_symlink():
            raise RpcError(-32002, 'build is incomplete or unknown')
        report = decode(self.read_file(report_path))
        allowed = {'report.json', *report['artifacts'].values(), *output_files(report)}
        path = directory / artifact
        if artifact not in allowed or path.is_symlink() or not path.resolve().is_relative_to(self.output_root):
            raise RpcError(-32002, 'unknown build artifact')
        return self.artifact_content(uri, path)

    def artifact_content(self, uri, path):
        data = self.read_file(path)
        mime = {'.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.html': 'text/html', '.dxf': 'image/vnd.dxf', '.stl': 'model/stl', '.step': 'application/step'}.get(path.suffix, 'application/octet-stream')
        content = dict(uri=uri, mimeType=mime)
        if path.suffix in ('.json', '.svg', '.html', '.dxf'):
            content['text'] = data.decode('utf-8')
        else:
            content['blob'] = base64.b64encode(data).decode('ascii')
        return {'contents': [content]}

    def inline_renders(self, directory, report):
        """Render PNGs to attach in report order, within the inline budget."""
        images, names, total = [], [], 0
        for output in report['outputs']:
            for image in output.get('renders', ()):
                name = image.get('file')
                path = directory / name if isinstance(name, str) else None
                if path is None or len(images) == MAX_INLINE_IMAGES or path.is_symlink() or not path.is_file():
                    continue
                data = self.read_file(path)
                if total + len(data) > MAX_INLINE_BYTES:
                    continue
                total += len(data)
                images.append(data)
                names.append(name)
        return images, names

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

    async def edit_build(self, arguments, preview=False):
        allowed = set(self.build_edit_schema['properties'])
        if not isinstance(arguments, dict) or not set(arguments).issubset(allowed) or not isinstance(arguments.get('build_id'), str) or not re.fullmatch(r'[0-9a-f]{32}', arguments['build_id']) or not isinstance(arguments.get('expected_model_sha256'), str) or not re.fullmatch(r'[0-9a-f]{64}', arguments['expected_model_sha256']):
            raise RpcError(-32602, 'expected build_id, expected_model_sha256, and supported edit fields')
        text = self.accepted_model(arguments['build_id'])
        fingerprint = hashlib.sha256(text.encode('utf-8')).hexdigest()
        if fingerprint != arguments['expected_model_sha256']:
            return tool_result(dict(status='failed', stage='conflict', message='source model fingerprint differs from the inspected snapshot'), True)
        request = {key: value for key, value in arguments.items() if key not in ('build_id', 'expected_model_sha256')}
        request.update(schema='occb-model-edit-v1', model=decode(text), source=dict(build_id=arguments['build_id'], model_sha256=fingerprint))
        return await self.worker(request, editing=True, previewing=preview)

    async def worker(self, request, inspection=False, editing=False, visualization=False, previewing=False):
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
            elif previewing:
                command.append('--preview-edit')
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
            filenames = {'report.json', *report['artifacts'].values(), *output_files(report)}
            with (work / 'build/model.json').open('rb') as file:
                fingerprint = hashlib.file_digest(file, 'sha256').hexdigest()
            accepted = True
            result = dict(model_sha256=fingerprint, report=report, directory=str(work / 'build'),resources={name: f'occt://build/{build_id}/{name}' for name in sorted(filenames)})
            result['visualization_id' if visualization or previewing else 'build_id'] = build_id
            images, attached = self.inline_renders(work / 'build', report)
            if attached:
                result['inline_images'] = attached
            return tool_result(result, images=images)
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

    def studio_url(self, arguments, allowed):
        if not isinstance(arguments, dict) or not set(arguments).issubset(allowed):
            raise RpcError(-32602, f'expected only {", ".join(sorted(allowed))}')
        return studio_base(arguments['url']) if 'url' in arguments else self.studio

    async def studio_request(self, base, path, body=None):
        """(status, JSON reply) from the studio, or None when it cannot be reached."""
        def call():
            data = None if body is None else encode(body).encode('utf-8')
            request = urllib.request.Request(base + path, data=data, method='GET' if body is None else 'POST',
                headers={'Content-Type': 'application/json', 'X-OCCT-View': '1'})
            try:
                with urllib.request.urlopen(request, timeout=self.timeout) as response:
                    return response.status, decode(response.read(MAX_RESOURCE + 1))
            except urllib.error.HTTPError as error:
                try:
                    return error.code, decode(error.read(1024 * 1024))
                except ValueError:
                    return error.code, dict(error=error.reason)
            except (urllib.error.URLError, OSError, ValueError):
                return None
        return await asyncio.to_thread(call)

    def studio_failure(self, base, reply):
        if reply is None:
            return tool_result(dict(status='failed', stage='connection', message=f'no studio session at {base}; start occt-view MODEL --serve'), True)
        status, value = reply
        stage = {409: 'conflict', 400: 'request', 422: 'studio'}.get(status, 'studio')
        return tool_result(dict(status='failed', stage=stage, message=str(value.get('error', 'studio request failed')) if isinstance(value, dict) else 'studio request failed'), True)

    async def session_read(self, arguments):
        base = self.studio_url(arguments, {'url'})
        reply = await self.studio_request(base, '/api/session')
        if reply is None or reply[0] != 200:
            return self.studio_failure(base, reply)
        value = reply[1]
        value['url'] = base
        return tool_result(value)

    async def session_apply(self, arguments):
        base = self.studio_url(arguments, {'build_id', 'expected_model_sha256', 'message', 'author', 'url'})
        if not isinstance(arguments.get('build_id'), str) or not re.fullmatch(r'[0-9a-f]{32}', arguments['build_id']) \
                or not isinstance(arguments.get('expected_model_sha256'), str) or not re.fullmatch(r'[0-9a-f]{64}', arguments['expected_model_sha256']) \
                or not isinstance(arguments.get('message'), str) or not isinstance(arguments.get('author', 'AI assistant'), str):
            raise RpcError(-32602, 'expected build_id, expected_model_sha256 and message')
        # Only accepted (verified) builds reach the studio.
        model = decode(self.accepted_model(arguments['build_id']))
        reply = await self.studio_request(base, '/api/session/apply', dict(expected_model_sha256=arguments['expected_model_sha256'],
            model=model, author=arguments.get('author', 'AI assistant'), message=arguments['message']))
        if reply is None or reply[0] != 200:
            return self.studio_failure(base, reply)
        value = reply[1]
        value.update(url=base, build_id=arguments['build_id'])
        return tool_result(value)

    async def command_worker(self, name, request):
        suffix, report_name, single_file = COMMANDS[name]
        merging = suffix == 'document-merge'
        fields = {'schema', 'base', 'current', 'other'} if merging else {'schema', 'model', 'setup'}
        if not isinstance(request, dict) or set(request) != fields or request.get('schema') != f'occb-{suffix}-request-v1':
            raise RpcError(-32602, 'expected the complete generated command request; filesystem paths are not accepted')
        if not all(isinstance(request[k], dict) for k in fields - {'schema'}):
            raise RpcError(-32602, 'models and setup must be objects')
        if suffix == 'balance-report':
            reference = request['setup'].get('reference')
            if not isinstance(reference, dict) or reference.get('kind') != 'chord':
                raise RpcError(-32602, 'MCP balance reports require an inline chord reference')
        identity = uuid.uuid4().hex
        work = self.output_root / identity
        work.mkdir(mode=0o700)
        results = work / 'results'
        process = None
        retained = False
        try:
            command = [str(self.commands[name])]
            if merging:
                command.append('--report')
                for key in ('base', 'current', 'other'):
                    path = work / (key + '.json')
                    path.write_text(encode(request[key]))
                    command.append(str(path))
            else:
                for key in ('model', 'setup'):
                    path = work / (key + '.json')
                    path.write_text(encode(request[key]))
                    command.append(str(path))
            if single_file:
                results.mkdir()
            report_path = results / report_name
            command.append(str(report_path if single_file else results))
            with (work / 'kernel.stderr').open('wb') as stderr:
                process = await asyncio.create_subprocess_exec(*command,
                    stdin=asyncio.subprocess.DEVNULL, stdout=asyncio.subprocess.DEVNULL, stderr=stderr)
                await asyncio.wait_for(process.wait(), self.timeout)
            # Exit 2 is a completed negative measurement, motion check or merge.
            if process.returncode not in (0, 2) or not report_path.is_file():
                with (work / 'kernel.stderr').open('rb') as stderr:
                    stderr.seek(0, 2)
                    stderr.seek(max(0, stderr.tell() - 8192))
                    detail = stderr.read().decode('utf-8', errors='replace').strip()
                return tool_result(dict(status='failed', stage=suffix, message=detail or 'command failed without a report'), True)
            report = decode(self.read_file(report_path))
            files = sorted(p.name for p in results.iterdir() if p.is_file() and not p.is_symlink()
                           and re.fullmatch(r'[a-zA-Z0-9_.-]+', p.name))
            (work / 'analysis-artifacts.json').write_text(encode(files))
            retained = True
            return tool_result(dict(analysis_id=identity, command=suffix,
                status='completed' if process.returncode == 0 else 'failed_checks',
                report=report, directory=str(results),
                resources={file: f'occt://analysis/{identity}/{file}' for file in files}), process.returncode != 0)
        except TimeoutError:
            return tool_result(dict(status='failed', stage='timeout', message=f'command exceeded {self.timeout:g} seconds'), True)
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
            if not retained:
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
                        instructions='Read occt://catalog for current capabilities, then a schema/example. Design command tools return analysis artifacts; build resulting models separately to verify requirements. Preserve requirements and explicit assumptions, then build. Inspect sketch_feature for solver/repair evidence or loft_feature for isolated correspondence diagnostics, preview guarded proposals with occt_preview_edit_build, then publish intended changes with occt_edit_build. Read model.json to continue from the accepted model. Preview SVGs are sampled; inspect requirement evidence. To work with a person in the studio, occt_session_read the shared model, build or edit it, then occt_session_apply the accepted build with the read model_sha256.')
        if self.state != 'ready':
            raise RpcError(-32600, 'initialize and send notifications/initialized first')
        if method == 'tools/list':
            if params.get('cursor'):
                raise RpcError(-32602, 'no pagination cursor is supported')
            return dict(tools=self.tools())
        if method == 'tools/call':
            name, arguments = params.get('name'), params.get('arguments', {})
            if name in self.commands:
                return await self.command_worker(name, arguments)
            if name == 'occt_get_schema':
                return tool_result(self.schemas[self.named(arguments, self.schemas)])
            if name == 'occt_get_example':
                return tool_result(self.examples[self.named(arguments, EXAMPLES)])
            if name == 'occt_inspect_model':
                return await self.inspect_model(arguments)
            if name == 'occt_inspect_build':
                return await self.inspect_build(arguments)
            if name == 'occt_visualize_model':
                return await self.worker(arguments, visualization=True)
            if name == 'occt_preview_edit_build':
                return await self.edit_build(arguments, preview=True)
            if name == 'occt_edit_build':
                return await self.edit_build(arguments)
            if name == 'occt_build':
                return await self.build(arguments)
            if name == 'occt_session_read':
                return await self.session_read(arguments)
            if name == 'occt_session_apply':
                return await self.session_apply(arguments)
            raise RpcError(-32602, 'unknown tool')
        if method == 'resources/list':
            return dict(resources=self.fixed_resources())
        if method == 'resources/templates/list':
            return dict(resourceTemplates=[dict(uriTemplate='occt://analysis/{analysis_id}/{artifact}', name='Design command artifact',
                description='Only artifact URIs returned by design tools can be read; reports, models, SVG and DXF; maximum 32 MiB.'), dict(uriTemplate='occt://build/{build_id}/{artifact}', name='Build or diagnostic artifact',
                description='Use artifact URIs returned by build or preview tools: model/report JSON, SVG previews, binary STL or STEP; maximum read size 32 MiB.')])
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
    parser.add_argument('--timeout', type=float, default=120, help='per-worker timeout in seconds (default 120)')
    parser.add_argument('--commands-dir', help='directory of optional design command binaries (default: model binary directory)')
    parser.add_argument('--studio-url', help=f'local studio for shared sessions (default: $OCCT_STUDIO_URL or {DEFAULT_STUDIO})')
    args = parser.parse_args()
    if not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error('--timeout must be finite and positive')
    asyncio.run(Server(args.binary, args.output_root, args.timeout, args.commands_dir, args.studio_url).serve())

if __name__ == '__main__':
    main()
