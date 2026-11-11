import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.error
import sys

sys.path.insert(0, str(Path(__file__).parent))
from cloudflare_api import Cloudflare, DeploymentError


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class PackagingTests(unittest.TestCase):
    def test_package(self):
        module = load('package-web')
        with tempfile.TemporaryDirectory() as temp:
            source, output = Path(temp) / 'source', Path(temp) / 'output'
            source.mkdir()
            (source / 'index.html').write_text('canvas, noInitialRun:true, <script src="fatherload.js"></script>')
            for name in ('fatherload.js', 'fatherload.wasm'):
                (source / name).write_bytes(name.encode())
            module.package(source, output, 'revision')
            html = (output / 'index.html').read_text()
            self.assertIn('locateFile(path)', html)
            self.assertNotIn('src="fatherload.js"', html)
            self.assertEqual(len(list((output / 'assets').iterdir())), 2)
            self.assertTrue((output / '404.html').exists())
            with self.assertRaises(ValueError):
                module.package(source, output, 'revision')

    def test_reject_changed_loader(self):
        module = load('package-web')
        with tempfile.TemporaryDirectory() as temp:
            source = Path(temp)
            (source / 'index.html').write_text('changed loader')
            for name in ('fatherload.js', 'fatherload.wasm'):
                (source / name).write_bytes(b'test')
            with self.assertRaises(ValueError):
                module.package(source, source / 'output', 'revision')


class ProviderTests(unittest.TestCase):
    def test_error_does_not_disclose_credentials(self):
        with patch.dict('os.environ', {'CLOUDFLARE_API_TOKEN': 'secret-token'}):
            cf = Cloudflare()
        error = urllib.error.HTTPError('https://secret-url', 403, 'secret-response', {}, None)
        with patch('urllib.request.urlopen', side_effect=error):
            with self.assertRaises(DeploymentError) as raised:
                cf.request('POST', '/user/tokens', {'value': 'secret-body'})
        self.assertNotIn('secret', str(raised.exception))

    def test_conflicting_dns_not_modified(self):
        module = load('configure-pages')
        fake = unittest.mock.Mock()
        fake.zone.return_value = {'id': 'zone', 'account': {'id': 'account'}}
        fake.request.side_effect = [[{'name': 'fatherload', 'production_branch': 'master'}],
                                    [{'type': 'A', 'content': '192.0.2.1'}]]
        with patch.object(module, 'Cloudflare', return_value=fake), patch.dict('os.environ', {'CLOUDFLARE_ACCOUNT_ID': 'account'}):
            with self.assertRaises(DeploymentError):
                module.configure(True)
        self.assertTrue(all(call.args[0] == 'GET' for call in fake.request.call_args_list))

    def test_secret_skip_does_not_create_token(self):
        module = load('provision-production')
        outputs = ['{"id":1524311}', '{}', '{"branch_policies":[{"name":"master","type":"branch"}]}',
                   '[{"name":"CLOUDFLARE_API_TOKEN"},{"name":"CLOUDFLARE_ACCOUNT_ID"}]']
        with patch.object(module, 'command', side_effect=outputs), patch.object(module, 'Cloudflare') as cf:
            module.provision()
            cf.assert_not_called()


if __name__ == '__main__':
    unittest.main()
