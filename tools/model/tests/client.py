"""Small wire client used only by MCP tests and benchmarks."""
import json
import os
import pathlib
import select
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
BINARY = pathlib.Path(os.environ.get('OCCT_MODEL_BINARY', ROOT / 'rust/occt-parametric/target/debug/occt-model')).resolve()
SERVER = ROOT / 'tools/model/mcp_server.py'

class Client:
    def __init__(self, root, binary=BINARY, timeout=120, initialize=True):
        self.process = subprocess.Popen([sys.executable, str(SERVER), '--binary', str(binary), '--output-root', str(root), '--timeout', str(timeout)],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.counter = 0
        if initialize:
            self.send(dict(jsonrpc='2.0', id=0, method='initialize', params=dict(protocolVersion='2025-11-25', capabilities={}, clientInfo=dict(name='test', version='1'))))
            initialized = self.receive()
            assert initialized['result']['protocolVersion'] == '2025-11-25'
            self.send(dict(jsonrpc='2.0', method='notifications/initialized'))

    def send(self, value):
        self.process.stdin.write(json.dumps(value).encode() + b'\n')
        self.process.stdin.flush()

    def receive(self, timeout=20):
        ready = select.select([self.process.stdout], [], [], timeout)[0]
        if not ready:
            raise TimeoutError('MCP response did not arrive')
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError(self.process.stderr.read().decode())
        return json.loads(line)

    def call(self, method, params=None):
        self.counter += 1
        self.send(dict(jsonrpc='2.0', id=self.counter, method=method, params=params or {}))
        message = self.receive()
        assert message['id'] == self.counter, message
        return message

    def tool(self, name, arguments):
        return self.call('tools/call', dict(name=name, arguments=arguments))['result']

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.process.stdout.close()
        self.process.stderr.close()

