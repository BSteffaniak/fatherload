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
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                payload = json.load(response)
            if not payload.get('success'):
                raise DeploymentError('Cloudflare rejected the operation; response suppressed')
            return payload['result']
        except urllib.error.HTTPError as error:
            status = error.code
            error.close()
            raise DeploymentError(f'Cloudflare HTTP {status}; response suppressed. Inspect provider state before retrying mutations.') from None
        except (urllib.error.URLError, OSError, ValueError, KeyError):
            raise DeploymentError('Cloudflare response unavailable or invalid; inspect provider state before retrying mutations') from None

    def zone(self):
        zones = self.request('GET', '/zones?name=hyperchad.dev')
        if len(zones) != 1 or zones[0]['name'] != 'hyperchad.dev':
            raise DeploymentError('Expected exactly one hyperchad.dev zone')
        return zones[0]
