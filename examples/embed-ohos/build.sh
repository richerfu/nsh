#!/usr/bin/env bash
# Assemble the example with a previously built native SDK and nsh CLI.
set -euo pipefail
APP="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$APP/../.." && pwd)"
PROFILE="${NSH_PROFILE:-debug}"
SDK="$ROOT/target/ohos/$PROFILE"
export NSH_PROFILE="$PROFILE"
export NSH_SDK_METADATA="$SDK/build-metadata.json"
: "${OHOS_NDK_HOME:?Set OHOS_NDK_HOME to the SDK openharmony directory}"
: "${DEVECO_CONTENTS:?Set DEVECO_CONTENTS to DevEco-Studio.app/Contents}"
export DEVECO_SDK_HOME="$DEVECO_CONTENTS/sdk"
export PATH="$DEVECO_CONTENTS/tools/node/bin:$PATH"
python3 "$APP/prepare-ability.py"
mkdir -p "$APP/entry/libs/arm64-v8a"
cp "$SDK/arm64-v8a/libnsh_platform_ohos.so" "$APP/entry/libs/arm64-v8a/"
cp "$SDK/index.d.ts" "$APP/entry/src/main/cpp/types/libnsh_platform_ohos/Index.d.ts"
cp "$OHOS_NDK_HOME/native/llvm/lib/aarch64-linux-ohos/libc++_shared.so" "$APP/entry/libs/arm64-v8a/"
BUILD_FLAGS=()
if [[ "${NSH_SPLIT:-0}" == 1 ]]; then BUILD_FLAGS+=(--split); fi
"${NSH_CLI:-$ROOT/target/debug/nsh}" build "$ROOT/${NSH_EXAMPLE:-examples/counter}" -o "$APP/entry/src/main/resources/rawfile" "${BUILD_FLAGS[@]}"
# Device fixtures can supply HTML and other raw resources alongside imports.
FIXTURE_RESOURCES="$ROOT/${NSH_EXAMPLE:-examples/counter}/resources"
if [[ -d "$FIXTURE_RESOURCES" ]]; then
  mkdir -p "$APP/entry/src/main/resources/rawfile/assets"
  cp -R "$FIXTURE_RESOURCES/." "$APP/entry/src/main/resources/rawfile/assets/"
fi
cd "$APP"
MODULE_BACKUP=""
BADGE_BACKUP=""
INDEX_BACKUP=""
PROFILE_BACKUP=""
ENTRY_PROFILE_BACKUP=""
restore_fixtures() {
  local fixture_status=$?
  if [[ -n "$MODULE_BACKUP" ]]; then
    if cp "$MODULE_BACKUP" "$APP/entry/src/main/module.json5"; then
      rm -f "$MODULE_BACKUP" "$APP/entry/src/main/ets/entryability/AccessibilityE2ETestExtension.ets" "$APP/entry/src/main/resources/base/profile/accessibility_e2e_config.json" || fixture_status=$?
    else
      fixture_status=$?
      printf 'Module fixture restore failed; backup retained at %s\n' "$MODULE_BACKUP" >&2
    fi
  fi
  if [[ -n "$BADGE_BACKUP" ]]; then
    if cp "$BADGE_BACKUP" "$APP/entry/src/main/ets/components/HostBadge.ets"; then
      rm -f "$BADGE_BACKUP" || fixture_status=$?
    else
      fixture_status=$?
      printf 'Native pointer fixture restore failed; backup retained at %s\n' "$BADGE_BACKUP" >&2
    fi
  fi
  if [[ -n "$INDEX_BACKUP" ]]; then
    if cp "$INDEX_BACKUP" "$APP/entry/src/main/ets/pages/Index.ets"; then
      rm -f "$INDEX_BACKUP" || fixture_status=$?
    else
      fixture_status=$?
      printf 'Dev overlay fixture restore failed; backup retained at %s\n' "$INDEX_BACKUP" >&2
    fi
  fi
  for profile_pair in "$PROFILE_BACKUP:$APP/build-profile.json5" "$ENTRY_PROFILE_BACKUP:$APP/entry/build-profile.json5"; do
    local profile_copy="${profile_pair%%:*}"
    if [[ -n "$profile_copy" ]]; then
      if cp "$profile_copy" "${profile_pair#*:}"; then
        rm -f "$profile_copy" || fixture_status=$?
      else
        fixture_status=$?
        printf 'Build profile restore failed; backup retained at %s\n' "$profile_copy" >&2
      fi
    fi
  done
  exit "$fixture_status"
}
trap restore_fixtures EXIT
profile_copy="$(mktemp)"
if ! cp "$APP/build-profile.json5" "$profile_copy"; then rm -f "$profile_copy"; exit 1; fi
PROFILE_BACKUP="$profile_copy"
profile_copy="$(mktemp)"
if ! cp "$APP/entry/build-profile.json5" "$profile_copy"; then rm -f "$profile_copy"; exit 1; fi
ENTRY_PROFILE_BACKUP="$profile_copy"
python3 - <<'PY_METADATA'
import json
from sdk_manifest import load
import os
from pathlib import Path
metadata = json.loads(Path(os.environ['NSH_SDK_METADATA']).read_text())
version = {20: '6.0.0(20)', 23: '6.1.0(23)', 26: '26.0.0'}[metadata['minimumApi']]
profile = Path('build-profile.json5')
value = load(profile)
for product in value['app']['products']:
    product['compatibleSdkVersion'] = version
    product['targetSdkVersion'] = version
    if os.environ.get('NSH_UNSIGNED') == '1':
        product.pop('signingConfig', None)
if os.environ.get('NSH_UNSIGNED') == '1':
    value['app'].pop('signingConfigs', None)
profile.write_text(json.dumps(value, indent=2) + '\n')
profile = Path('entry/build-profile.json5')
value = load(profile)
# Preserve the built library byte-for-byte so the SDK/HAP identity is verifiable.
value.setdefault('buildOption', {}).setdefault('nativeLib', {}).setdefault('debugSymbol', {})['strip'] = False
profile.write_text(json.dumps(value, indent=2) + '\n')
PY_METADATA
# The optional QEMU service belongs to the acceptance HAP, not the embedding SDK.
if [[ "${NSH_A11Y_E2E:-0}" == 1 || "${NSH_CLIPBOARD_E2E:-0}" == 1 ]]; then
  module_copy="$(mktemp)"
  if ! cp "$APP/entry/src/main/module.json5" "$module_copy"; then
    rm -f "$module_copy"
    exit 1
  fi
  MODULE_BACKUP="$module_copy"
fi
if [[ "${NSH_NATIVE_POINTER_E2E:-0}" == 1 ]]; then
  badge_copy="$(mktemp)"
  if ! cp "$APP/entry/src/main/ets/components/HostBadge.ets" "$badge_copy"; then
    rm -f "$badge_copy"
    exit 1
  fi
  BADGE_BACKUP="$badge_copy"
  cp "$APP/tests/native-pointer/HostBadge.ets" "$APP/entry/src/main/ets/components/HostBadge.ets"
fi
if [[ "${NSH_DEV_OVERLAY_E2E:-0}" == 1 ]]; then
  index_copy="$(mktemp)"
  if ! cp "$APP/entry/src/main/ets/pages/Index.ets" "$index_copy"; then
    rm -f "$index_copy"
    exit 1
  fi
  INDEX_BACKUP="$index_copy"
  cp "$APP/tests/dev-overlay/Index.ets" "$APP/entry/src/main/ets/pages/Index.ets"
fi
if [[ "${NSH_A11Y_E2E:-0}" == 1 ]]; then
  cp "$APP/tests/accessibility/AccessibilityE2ETestExtension.ets" "$APP/entry/src/main/ets/entryability/"
  cp "$APP/tests/accessibility/accessibility_e2e_config.json" "$APP/entry/src/main/resources/base/profile/"
  python3 - <<'PY'
import json
from sdk_manifest import load
from pathlib import Path
p = Path('entry/src/main/module.json5')
s = load(p)
s['module']['extensionAbilities'] = [{
  'name': 'AccessibilityE2ETestExtension',
  'srcEntry': './ets/entryability/AccessibilityE2ETestExtension.ets',
  'label': '$string:app_name', 'description': '$string:app_name',
  'type': 'accessibility',
  'metadata': [{'name': 'ohos.accessibleability', 'resource': '$profile:accessibility_e2e_config'}],
}]
p.write_text(json.dumps(s, indent=2) + '\n')
PY
fi
if [[ "${NSH_CLIPBOARD_E2E:-0}" == 1 ]]; then
  python3 - <<'PY'
import json
from sdk_manifest import load
from pathlib import Path
p = Path('entry/src/main/module.json5')
s = load(p)
permissions = s['module'].setdefault('requestPermissions', [])
if not any(item['name'] == 'ohos.permission.READ_PASTEBOARD' for item in permissions):
    permissions.append({
        'name': 'ohos.permission.READ_PASTEBOARD',
        'reason': '$string:clipboard_permission_reason',
        'usedScene': {'abilities': ['EntryAbility'], 'when': 'inuse'},
    })
p.write_text(json.dumps(s, indent=2) + '\n')
PY
fi
"$DEVECO_CONTENTS/tools/ohpm/bin/ohpm" install
export NSH_HAP_BUILD_START="$(date +%s)"
"$DEVECO_CONTENTS/tools/hvigor/bin/hvigorw" assembleHap --no-daemon --mode module -p product=default -p buildMode=debug --no-hvigorw-daemon

python3 - <<'PY_PACKAGE'
import hashlib
import json
from sdk_manifest import load
import os
from pathlib import Path
import zipfile
metadata = json.loads(Path(os.environ['NSH_SDK_METADATA']).read_text())
reports = []
for hap in sorted(Path('entry/build/default/outputs/default').glob('*.hap')):
    if hap.stat().st_mtime < int(os.environ['NSH_HAP_BUILD_START']):
        continue
    with zipfile.ZipFile(hap) as package:
        name = 'libs/arm64-v8a/libnsh_platform_ohos.so'
        module = json.loads(package.read('module.json'))
        expected_api = {20: 60000020, 23: 60100023, 26: 260000026}[metadata['minimumApi']]
        if module['app']['minAPIVersion'] != expected_api:
            raise SystemExit('Packaged minimum API differs from SDK metadata')
        digest = hashlib.sha256(package.read(name)).hexdigest()
        if digest != metadata['librarySha256']:
            raise SystemExit('Packaged native library differs from selected SDK: ' + str(hap))
        reports.append({'hap': str(hap), 'sha256': hashlib.sha256(hap.read_bytes()).hexdigest(), 'librarySha256': digest, 'minimumApi': metadata['minimumApi'], 'abilitySource': metadata['abilitySource'], 'bindingSource': metadata['bindingSource']})
if not reports:
    raise SystemExit('Hvigor produced no HAP')
Path('entry/build/paired-haps.json').write_text(json.dumps(reports, indent=2) + '\n')
print('Verified packaged SDK identity for ' + str(len(reports)) + ' HAP(s)')
PY_PACKAGE
