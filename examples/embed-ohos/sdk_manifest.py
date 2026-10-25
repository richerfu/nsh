"""Read SDK JSON5 manifests through DevEco's bundled JSON5 parser."""
import json
import os
from pathlib import Path
import subprocess


def load(path):
    contents = Path(os.environ['DEVECO_CONTENTS'])
    result = subprocess.run([
        contents / 'tools/node/bin/node', '-e',
        "process.stdout.write(JSON.stringify(require(process.argv[1]).parse(require('fs').readFileSync(process.argv[2], 'utf8'))))",
        str(contents / 'tools/hvigor/hvigor-ohos-plugin/node_modules/json5'), str(path),
    ], capture_output=True, text=True)
    if result.returncode:
        raise ValueError('Cannot parse JSON5 manifest: ' + str(path))
    return json.loads(result.stdout)
