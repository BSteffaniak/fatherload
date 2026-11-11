#!/usr/bin/env python3
"""Idempotently provision only Fatherload Pages/DNS; refuse conflicting DNS records."""
import argparse
import os
import sys
from cloudflare_api import Cloudflare, DeploymentError

PROJECT = 'fatherload'
DOMAIN = 'fatherload.hyperchad.dev'


def configure(domains):
    cf = Cloudflare()
    zone = cf.zone()
    account = os.environ.get('CLOUDFLARE_ACCOUNT_ID')
    if not account or account != zone['account']['id']:
        raise DeploymentError('CLOUDFLARE_ACCOUNT_ID must match the hyperchad.dev account')
    root = f'/accounts/{account}/pages/projects'
    # List before creation: do not retry an uncertain mutation as a blind create.
    projects = []
    page = 1
    while True:
        batch = cf.request('GET', root + f'?page={page}&per_page=100')
        projects.extend(batch)
        if len(batch) < 100:
            break
        page += 1
    project = next((p for p in projects if p['name'] == PROJECT), None)
    if project is None:
        if domains:
            raise DeploymentError('Pages project missing; run the project setup first')
        cf.request('POST', root, {'name': PROJECT, 'production_branch': 'master'})
        print('Created fatherload Pages project')
    elif project['production_branch'] != 'master':
        raise DeploymentError('Existing Pages project has an unexpected production branch')
    if not domains:
        return
    base = root + '/' + PROJECT
    records = cf.request('GET', f'/zones/{zone["id"]}/dns_records?name={DOMAIN}')
    target = PROJECT + '.pages.dev'
    if records and (len(records) != 1 or records[0]['type'] != 'CNAME' or records[0]['content'].rstrip('.') != target):
        raise DeploymentError('Conflicting fatherload DNS record; left untouched')
    attached = cf.request('GET', base + '/domains')
    if not any(d['name'] == DOMAIN for d in attached):
        cf.request('POST', base + '/domains', {'name': DOMAIN})
        print('Attached Fatherload custom domain')
    if not records:
        cf.request('POST', f'/zones/{zone["id"]}/dns_records', {
            'type': 'CNAME', 'name': DOMAIN, 'content': target, 'proxied': True, 'ttl': 1})
        print('Created Fatherload CNAME')
    print('Pages/DNS configuration verified; certificate activation may take time')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--domains', action='store_true', help='Attach DNS/domain after the first successful deployment')
    args = parser.parse_args()
    try:
        configure(args.domains)
    except (DeploymentError, KeyError, TypeError, ValueError) as error:
        print(str(error) if isinstance(error, DeploymentError) else 'Unexpected provider response; inspect state before retrying', file=sys.stderr)
        sys.exit(1)
