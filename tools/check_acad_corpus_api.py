#!/usr/bin/env python3
"""Require the retained drawing corpus and exercise native stdio MCP workflows."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import tempfile
import tomllib

from check_acad_gui_api import Mcp


def fingerprint(state):
    return {key: state[key] for key in ('entities', 'selectable_objects', 'blocks')}


def check_drawing(source, binary, directories, scratch, log):
    original_bytes = source.read_bytes()
    args = ['--drawing', source]
    for directory in directories:
        args += ['--fonts', directory]
    process = Mcp(binary, args, log)
    try:
        state = process.tool('state')['structuredContent']
        assert not state['dirty'], state
        source_format = state['format']
        original_drawing = process.tool('drawing')['structuredContent']
        for value in ['MENU', '', 'ZOOM', 'A']:
            process.tool('command', input=value)
        drawing = process.tool('drawing')['structuredContent']
        state = process.tool('state')['structuredContent']
        counts = fingerprint(state)
        frame = process.tool('frame', width=800, height=600, format='rgba')['structuredContent']
        assert not frame['diagnostics'], frame['diagnostics']
        rgba = base64.b64decode(frame['data'], validate=True)
        assert len(rgba) == 800 * 600 * 4, len(rgba)
        # MENU is unloaded: omit only the bottom command/status panel.
        lit = sum(any(rgba[i:i + 3]) for i in range(0, 800 * 552 * 4, 4))
        assert lit > 0, f'{source.name}: empty drawing canvas'
        png_result = process.tool('frame', width=800, height=600, format='png')
        assert not png_result['structuredContent']['diagnostics']
        png = base64.b64decode(png_result['content'][0]['data'], validate=True)
        assert png.startswith(b'\x89PNG\r\n\x1a\n')
        (scratch / (source.name + '.png')).write_bytes(png)
        outputs = []
        dxf_changed_pixels = 0
        for suffix in ('.dwg', '.dxf'):
            output = scratch / (source.name + suffix)
            process.tool('save', path=str(output))
            assert not process.tool('state')['structuredContent']['dirty']
            assert output.is_file() and output.stat().st_size > 0
            if suffix == '.dwg':
                # Explicit Save As chooses AC1.40; END retains attached revision.
                assert output.read_bytes()[:6] == b'AC1.40', 'Save As DWG revision changed'
            reopened_args = ['--drawing', output]
            for directory in directories:
                reopened_args += ['--fonts', directory]
            reopened = Mcp(binary, reopened_args, log)
            try:
                reopened_state = reopened.tool('state')['structuredContent']
                assert not reopened_state['dirty'], reopened_state
                assert fingerprint(reopened_state) == counts, (counts, reopened_state)
                assert reopened.tool('drawing')['structuredContent'] == drawing, source.name
                # Same effective canvas/resources after reopening, regardless of status/path.
                for value in ['MENU', '']:
                    reopened.tool('command', input=value)
                result = reopened.tool('frame', width=800, height=600, format='rgba')['structuredContent']
                assert not result['diagnostics'], result['diagnostics']
                pixels = base64.b64decode(result['data'], validate=True)
                assert len(pixels) == len(rgba)
                if suffix == '.dwg':
                    assert pixels[:800 * 552 * 4] == rgba[:800 * 552 * 4], source.name
                else:
                    # Historical DXF rounds numbers to six decimal places and
                    # omits AXIS; exact original-pixel equality is not its contract.
                    dxf_changed_pixels = sum(pixels[i:i + 4] != rgba[i:i + 4]
                                             for i in range(0, 800 * 552 * 4, 4))
                    assert any(pixels[i:i + 3] != b'\0\0\0'
                               for i in range(0, 800 * 552 * 4, 4)), source.name
                assert reopened.tool('quit', discard=True)['structuredContent']['quit']
            finally:
                reopened.close()
            outputs.append(str(output))
        assert source.read_bytes() == original_bytes, 'Corpus source changed'
        attached = scratch / (source.name + '.attached')
        attached.write_bytes(original_bytes)
        process.tool('open', path=str(attached), directories=list(map(str, directories)))
        assert process.tool('state')['structuredContent']['format'] == source_format
        assert process.tool('command', input='END')['structuredContent']['quit']
        if source_format != 'dxf':
            assert attached.read_bytes()[:6] == original_bytes[:6], 'END changed DWG revision'
        end_args = ['--drawing', attached]
        for directory in directories:
            end_args += ['--fonts', directory]
        ended = Mcp(binary, end_args, log)
        try:
            assert ended.tool('drawing')['structuredContent'] == original_drawing
            assert not ended.tool('state')['structuredContent']['dirty']
            assert ended.tool('quit', discard=True)['structuredContent']['quit']
        finally:
            ended.close()
        return dict(source=str(source), format=source_format, **counts,
                    canvas_lit_pixels=lit, dxf_canvas_changed_pixels=dxf_changed_pixels,
                    saved=outputs)
    finally:
        process.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', type=Path, help='Built acad-mcp directory')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    binary = ((args.bin_dir or root / 'target/debug') / 'acad-mcp').resolve()
    manifest = tomllib.loads((root / 'corpus/manifest.toml').read_text())['file']
    # 20 DWGs plus DISC.BAK are the 21-drawing renderer corpus. Also require
    # the other three Samples backups and the known textual SUBDIV.DXF sibling.
    # SHUTTLE.DXF and System/ACAD.BAK are not asserted as supported drawings.
    entries = [entry for entry in manifest if entry['disk'] == 'Samples' and
               (entry['name'].endswith(('.DWG', '.BAK')) or entry['name'] == 'SUBDIV.DXF')]
    assert len(entries) == 25, f'Expected 25 corpus drawings/backups, found {len(entries)}'
    original_hashes = {}
    for entry in entries:
        source = root / 'corpus' / entry['disk'] / entry['name']
        data = source.read_bytes()  # Missing corpus is a failure, never a skipped pass.
        assert len(data) == entry['bytes'], source
        assert hashlib.sha256(data).hexdigest() == entry['sha256'], source
        original_hashes[source] = entry['sha256']
    directories = [root / 'corpus/System', root / 'corpus/Samples']
    assert all(directory.is_dir() for directory in directories)
    scratch = Path(tempfile.mkdtemp(prefix='acad-corpus-api-'))
    print(f'Artifacts: {scratch}', flush=True)
    results = []
    with (scratch / 'mcp.log').open('w') as log:
        for entry in entries:
            source = root / 'corpus' / entry['disk'] / entry['name']
            result = check_drawing(source, binary, directories, scratch, log)
            results.append(result)
            (scratch / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
            print(f'PASS: {source.name}', flush=True)
    assert all(hashlib.sha256(path.read_bytes()).hexdigest() == value
               for path, value in original_hashes.items()), 'Corpus changed'
    print('PASS: all 21 corpus drawings, 3 additional backups and SUBDIV.DXF; '
          'MCP frames, Save As/END revisions, DWG canvas and canonical DXF save/reopen, source hashes')


if __name__ == '__main__':
    main()
