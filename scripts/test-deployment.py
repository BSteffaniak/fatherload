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


class VerificationTests(unittest.TestCase):
    def test_explicit_user_agent_and_safe_http_error(self):
        module = load('check-web-release')
        error = urllib.error.HTTPError('https://private-url', 403, 'private-response', {}, None)
        with patch.object(module.urllib.request, 'urlopen', side_effect=error) as opened:
            with self.assertRaises(urllib.error.HTTPError):
                module.verify('https://example.com', 'revision', None)
        request = opened.call_args.args[0]
        self.assertEqual(request.get_header('User-agent'), 'Fatherload-Release-Check/1.0')
        self.assertEqual(module.failure_reason(error), 'HTTP 403')
        self.assertEqual(module.failure_reason(ValueError('HTML integrity mismatch')), 'HTML integrity mismatch')
        self.assertNotIn('private', module.failure_reason(OSError('private-details')))


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

    def test_http_error_identifies_operation_and_codes_only(self):
        import io
        with patch.dict('os.environ', {'CLOUDFLARE_API_TOKEN': 'secret-token'}):
            cf = Cloudflare()
        error = urllib.error.HTTPError('https://secret-url', 400, 'secret-response', {},
            io.BytesIO(b'{"errors":[{"code":1000,"message":"secret-token secret-body"}]}'))
        with patch('urllib.request.urlopen', side_effect=error):
            with self.assertRaises(DeploymentError) as raised:
                cf.request('GET', '/accounts/private-account/pages/projects?secret-query')
        text = str(raised.exception)
        self.assertIn('list Pages projects: HTTP 400, codes 1000', text)
        for value in ('secret', 'private-account'):
            self.assertNotIn(value, text)

    def test_pages_pagination_uses_supported_size(self):
        module = load('configure-pages')
        fake = unittest.mock.Mock()
        fake.zone.return_value = {'id': 'zone', 'account': {'id': 'account'}}
        fake.request.side_effect = [
            [{'name': f'other-{i}', 'production_branch': 'master'} for i in range(10)],
            [{'name': 'fatherload', 'production_branch': 'master'}],
        ]
        with patch.object(module, 'Cloudflare', return_value=fake), patch.dict('os.environ', {'CLOUDFLARE_ACCOUNT_ID': 'account'}):
            module.configure(False)
        self.assertEqual(fake.request.call_args_list, [
            unittest.mock.call('GET', '/accounts/account/pages/projects?page=1&per_page=10'),
            unittest.mock.call('GET', '/accounts/account/pages/projects?page=2&per_page=10'),
        ])

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
