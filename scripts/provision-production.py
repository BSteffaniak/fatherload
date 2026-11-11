#!/usr/bin/env python3
"""Create Fatherload's protected GitHub environment and missing Pages secrets.

Requires gh authentication plus CLOUDFLARE_PROVISION_TOKEN with API Tokens Read/Edit,
Zone Read for hyperchad.dev, and permission to delegate account Pages Write and zone
DNS Write/Zone Read. Secret values are only passed to gh over stdin, never printed.
No PWMTF credentials, DNS records, or tokens are changed.
"""
import json
import subprocess
import sys
from cloudflare_api import Cloudflare, DeploymentError

REPO = 'BSteffaniak/fatherload'
ENVIRONMENT = 'Production'


def command(args, body=None):
    try:
        result = subprocess.run(args, input=body, capture_output=True, text=True, timeout=90, check=False)
    except (OSError, subprocess.TimeoutExpired):
        raise DeploymentError('Subprocess unavailable or timed out; inspect GitHub/provider state before retrying') from None
    if result.returncode:
        raise DeploymentError('GitHub operation failed; diagnostics suppressed to protect credentials')
    return result.stdout


def provision():
    endpoint = f'repos/{REPO}/environments/{ENVIRONMENT}'
    # Declare the approval/branch restriction before installing credentials. The owner
    # can approve their own workflow (otherwise this single-maintainer repo deadlocks).
    owner = json.loads(command(['gh', 'api', 'users/BSteffaniak']))['id']
    command(['gh', 'api', '--method', 'PUT', endpoint, '--input', '-'], json.dumps({
        'reviewers': [{'type': 'User', 'id': owner}], 'prevent_self_review': False,
        'deployment_branch_policy': {'protected_branches': False, 'custom_branch_policies': True}}))
    policies = json.loads(command(['gh', 'api', endpoint + '/deployment-branch-policies']))['branch_policies']
    if not any(p['name'] == 'master' and p.get('type', 'branch') == 'branch' for p in policies):
        command(['gh', 'api', '--method', 'POST', endpoint + '/deployment-branch-policies', '--input', '-'],
                json.dumps({'name': 'master', 'type': 'branch'}))
    if any(p['name'] != 'master' or p.get('type', 'branch') != 'branch' for p in policies):
        raise DeploymentError('Unexpected production branch policies; review them manually (not deleted)')
    secrets = {item['name'] for item in json.loads(command([
        'gh', 'secret', 'list', '--repo', REPO, '--env', ENVIRONMENT, '--json', 'name']))}
    missing = {'CLOUDFLARE_API_TOKEN', 'CLOUDFLARE_ACCOUNT_ID'} - secrets
    print('Production environment: owner approval required, master branch only')
    if not missing:
        print('Both Cloudflare secrets already exist; left unchanged')
        return
    cf = Cloudflare('CLOUDFLARE_PROVISION_TOKEN')
    zone = cf.zone()
    if 'CLOUDFLARE_ACCOUNT_ID' in missing:
        command(['gh', 'secret', 'set', 'CLOUDFLARE_ACCOUNT_ID', '--repo', REPO, '--env', ENVIRONMENT], zone['account']['id'])
        print('SET CLOUDFLARE_ACCOUNT_ID')
    if 'CLOUDFLARE_API_TOKEN' not in missing:
        return
    groups = cf.request('GET', '/user/tokens/permission_groups')
    policies = []
    for scope, resource, names in [
        ('com.cloudflare.api.account', f'com.cloudflare.api.account.{zone["account"]["id"]}', ['Pages Write']),
        ('com.cloudflare.api.account.zone', f'com.cloudflare.api.account.zone.{zone["id"]}', ['Zone Read', 'DNS Write']),
    ]:
        permissions = []
        for name in names:
            matches = [g for g in groups if g['name'] == name and scope in g.get('scopes', [])]
            if len(matches) != 1:
                raise DeploymentError(f'Cannot uniquely resolve permission {name}')
            permissions.append({'id': matches[0]['id']})
        policies.append({'effect': 'allow', 'resources': {resource: '*'}, 'permission_groups': permissions})
    created = cf.request('POST', '/user/tokens', {
        'name': 'github:BSteffaniak/fatherload:Production:pages', 'policies': policies})
    try:
        command(['gh', 'secret', 'set', 'CLOUDFLARE_API_TOKEN', '--repo', REPO, '--env', ENVIRONMENT], created['value'])
    except DeploymentError:
        raise DeploymentError('Token created but GitHub write not confirmed. Token retained; inspect provider/GitHub state before retrying. No existing token revoked.') from None
    print('SET CLOUDFLARE_API_TOKEN (Pages Write for account; DNS Write/Zone Read for hyperchad.dev)')


if __name__ == '__main__':
    try:
        provision()
    except (DeploymentError, KeyError, TypeError, ValueError) as error:
        print(str(error) if isinstance(error, DeploymentError) else 'Unexpected provider response; inspect state before retrying', file=sys.stderr)
        sys.exit(1)
