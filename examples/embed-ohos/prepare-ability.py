#!/usr/bin/env python3
"""Stage the exact Ability HAR sources paired with the selected native SDK."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
from sdk_manifest import load

root = Path(__file__).resolve().parents[2]
sdk = root / 'target/ohos' / os.environ.get('NSH_PROFILE', 'debug')
metadata = json.loads((sdk / 'build-metadata.json').read_text())
for relative, key in [('arm64-v8a/libnsh_platform_ohos.so', 'librarySha256'), ('index.d.ts', 'declarationsSha256')]:
    if hashlib.sha256((sdk / relative).read_bytes()).hexdigest() != metadata[key]:
        raise SystemExit('SDK artifact hash does not match build metadata: ' + relative)
if metadata['pathDevelopment'] and os.environ.get('NSH_ALLOW_PATH_DEPENDENCIES') != '1':
    raise SystemExit('Development path SDK requires explicit NSH_ALLOW_PATH_DEPENDENCIES=1')
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, default=root / 'target/ohos/arkts-deps')
target = parser.parse_args().output
frozen = os.environ.get('NSH_PAIRED_SOURCES')
if frozen:
    frozen = Path(frozen).resolve()
    inventory = json.loads((frozen / 'paired-sources.json').read_text())
    for key in ('abilitySource', 'abilityGitRevision', 'librarySha256', 'declarationsSha256'):
        if inventory.get(key) != metadata[key]:
            raise SystemExit('Frozen HAR source differs from SDK metadata: ' + key)
    for package in inventory['packages']:
        directory = Path(package['directory'])
        if directory.is_absolute() or len(directory.parts) != 1 or directory.name == '..':
            raise SystemExit('Invalid paired package directory')
        files = []
        for entry in package['files']:
            path = Path(entry['path'])
            if path.is_absolute() or '..' in path.parts:
                raise SystemExit('Invalid paired source path')
            data = (frozen / directory / path).read_bytes()
            if hashlib.sha256(data).hexdigest() != entry['sha256']:
                raise SystemExit('Frozen HAR source hash mismatch: ' + str(directory / path))
            files.append((path, data))
        destination = target / directory
        if destination.exists():
            shutil.rmtree(destination)
        for path, data in files:
            output = destination / path
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_bytes(data)
    target.mkdir(parents=True, exist_ok=True)
    (target / 'paired-sources.json').write_text(json.dumps(inventory, indent=2) + '\n')
    print('Staged verified frozen HAR sources from ' + str(frozen))
    raise SystemExit(0)
command = ['cargo', 'metadata', '--locked', '--format-version', '1', '--filter-platform', 'aarch64-unknown-linux-ohos']
if metadata['features']:
    command += ['--features', ','.join('nsh-platform-ohos/' + feature for feature in metadata['features'])]
graph = json.loads(subprocess.check_output(command, cwd=root))
ability = [package for package in graph['packages'] if package['name'] == 'openharmony-ability']
if len(ability) != 1 or (ability[0]['source'] or 'path') != metadata['abilitySource']:
    raise SystemExit('Ability Cargo source differs from the built native SDK')
source = Path(ability[0]['manifest_path']).parents[2]
revision = metadata['abilityGitRevision']
if revision and subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip() != revision:
    raise SystemExit('Ability HAR checkout revision differs from native SDK metadata')
packages = {'ability': source / 'native_ability', 'nsh': root / 'crates/nsh-platform-ohos/arkts'}
packages.update({name: source / 'plugins' / name for name in ('resource', 'clipboard', 'window', 'url', 'accessibility', 'webview')})
manifest_packages = []
for name, path in packages.items():
    destination = target / name
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(path, destination, ignore=shutil.ignore_patterns('build', '.hvigor', 'oh_modules', 'oh-package-lock.json5', 'BuildProfile.ets'))
    manifest = destination / 'oh-package.json5'
    content = load(manifest)
    if name != 'ability':
        content.setdefault('dependencies', {})['@ohos-rs/ability'] = 'file:../ability'
    manifest.write_text(json.dumps(content, indent=2) + '\n')
    files = []
    for staged in sorted(destination.rglob('*')):
        if staged.is_file():
            files.append({'path': staged.relative_to(destination).as_posix(), 'sha256': hashlib.sha256(staged.read_bytes()).hexdigest()})
    manifest_packages.append({'package': content['name'], 'directory': name, 'files': files})
(target / 'paired-sources.json').write_text(json.dumps({'abilitySource': metadata['abilitySource'], 'abilityGitRevision': revision, 'librarySha256': metadata['librarySha256'], 'declarationsSha256': metadata['declarationsSha256'], 'packages': manifest_packages}, indent=2) + '\n')
print('Staged paired Ability HAR revision ' + str(revision or 'explicit local development'))
