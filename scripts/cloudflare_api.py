#!/usr/bin/env python3
"""Small credential-safe Cloudflare API client for Fatherload deployment tools."""
import json
import os
import urllib.error
import urllib.request


class DeploymentError(Exception):
    """Safe-to-display operational failure."""


class Cloudflare:
    def __init__(self, credential='CLOUDFLARE_API_TOKEN'):
        self.token = os.environ.get(credential)
        if not self.token:
            raise DeploymentError(f'{credential} is required; supply it through a secure environment, not arguments')

    def request(self, method, path, body=None):
        request = urllib.request.Request(
            'https://api.cloudflare.com/client/v4' + path,
            data=None if body is None else json.dumps(body).encode(), method=method,
            headers={'Authorization': 'Bearer ' + self.token, 'Content-Type': 'application/json'})
        endpoint = path.split('?', 1)[0]
        if endpoint == '/zones':
            operation = 'discover hyperchad.dev zone'
        elif endpoint.endswith('/pages/projects'):
            operation = 'list Pages projects' if method == 'GET' else 'create Pages project'
        elif '/dns_records' in endpoint:
            operation = 'read DNS records' if method == 'GET' else 'create DNS record'
        elif endpoint.endswith('/domains'):
            operation = 'list custom domains' if method == 'GET' else 'attach custom domain'
        else:
            operation = 'Cloudflare API operation'

        def failure(status, payload):
            # Do not print arbitrary provider messages: they can echo request values.
            # Numeric codes and fixed operation labels are safe and actionable.
            codes = []
            if isinstance(payload, dict) and isinstance(payload.get('errors'), list):
                for error in payload['errors'][:10]:
                    if isinstance(error, dict) and type(error.get('code')) is int:
                        codes.append(str(error['code']))
            detail = ', codes ' + ', '.join(codes) if codes else ''
            return DeploymentError(
                f'{operation}: HTTP {status}{detail}. '
                'Provider text suppressed; inspect provider state before retrying mutations.')

        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                payload = json.load(response)
                status = response.status
            if not payload.get('success'):
                raise failure(status, payload)
            return payload['result']
        except urllib.error.HTTPError as error:
            try:
                payload = json.loads(error.read(65536))
            except (ValueError, OSError, TypeError):
                payload = {}
            finally:
                error.close()
            raise failure(error.code, payload) from None
        except (urllib.error.URLError, OSError, ValueError, KeyError):
            raise DeploymentError('Cloudflare response unavailable or invalid; inspect provider state before retrying mutations') from None

    def zone(self):
        zones = self.request('GET', '/zones?name=hyperchad.dev')
        if len(zones) != 1 or zones[0]['name'] != 'hyperchad.dev':
            raise DeploymentError('Expected exactly one hyperchad.dev zone')
        return zones[0]
