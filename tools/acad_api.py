#!/usr/bin/env python3
"""Call the native Rust window API; save PNG/RGBA frames without desktop capture."""
import argparse
import base64
import json
from pathlib import Path
import socket
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--socket', required=True)
    parser.add_argument('method', choices=['new', 'open', 'command', 'point', 'click',
                                        'state', 'drawing', 'save', 'cancel', 'report', 'frame', 'quit'])
    parser.add_argument('--params', default='{}', help='JSON object of method arguments')
    parser.add_argument('--output', type=Path, help='Write frame bytes to this file')
    args = parser.parse_args()
    try:
        params = json.loads(args.params)
        if not isinstance(params, dict):
            raise ValueError('--params must be a JSON object')
        if args.output and args.method != 'frame':
            raise ValueError('--output is only supported for frame')
        request = json.dumps({'method': args.method, 'params': params}).encode() + b'\n'
        if len(request) > 65536:
            raise ValueError('request exceeds 64 KiB')
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(35)
            client.connect(args.socket)
            client.sendall(request)
            with client.makefile('rb') as reader:
                response = json.loads(reader.readline(24 * 1024 * 1024))
        if 'error' in response:
            raise ValueError(response['error'])
        result = response['result']
        if args.output:
            data = base64.b64decode(result.pop('data'), validate=True)
            args.output.write_bytes(data)
            result['output'] = str(args.output)
        print(json.dumps(result, ensure_ascii=False, indent=2))
    except (OSError, ValueError, KeyError) as error:
        print(f'acad-api: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
