#!/usr/bin/env python3
"""Exercise a real native window through its socket and attached MCP (Unix)."""
import argparse
import base64
import json
import os
import re
from pathlib import Path
import selectors
import shutil
import socket
import subprocess
import sys
import tempfile
import time


def api(sock, method, *, expect_error=False, **params):
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(35)
        client.connect(str(sock))
        client.sendall((json.dumps(dict(method=method, params=params)) + '\n').encode())
        with client.makefile('rb') as reader:
            response = json.loads(reader.readline(24 * 1024 * 1024))
    if expect_error:
        assert 'error' in response, response
        return response['error']
    assert 'error' not in response, response
    return response['result']


class Mcp:
    def __init__(self, binary, args, log):
        self.child = subprocess.Popen([str(binary), *map(str, args)], stdin=subprocess.PIPE,
                                      stdout=subprocess.PIPE, stderr=log)
        self.counter = 0
        self.pending = bytearray()
        try:
            self.rpc('initialize', dict(protocolVersion='2025-11-25', capabilities={},
                                       clientInfo=dict(name='gui-api-smoke', version='1')))
            self.child.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
            self.child.stdin.flush()
        except Exception:
            self.close()
            raise

    def rpc(self, method, params):
        self.counter += 1
        self.child.stdin.write((json.dumps(dict(jsonrpc='2.0', id=self.counter,
                                               method=method, params=params)) + '\n').encode())
        self.child.stdin.flush()
        deadline = time.monotonic() + 35
        with selectors.DefaultSelector() as selector:
            selector.register(self.child.stdout, selectors.EVENT_READ)
            while b'\n' not in self.pending:
                remaining = deadline - time.monotonic()
                assert remaining > 0 and selector.select(remaining), 'MCP response timed out'
                data = self.child.stdout.read1(65536)
                assert data, 'MCP closed stdout before replying'
                self.pending.extend(data)
        line, _, tail = self.pending.partition(b'\n')
        self.pending = bytearray(tail)
        response = json.loads(line)
        assert response.get('id') == self.counter and 'error' not in response, response
        return response['result']

    def tool(self, name, **arguments):
        result = self.rpc('tools/call', dict(name='acad_' + name, arguments=arguments))
        assert not result.get('isError'), result
        return result

    def close(self):
        try:
            self.child.stdin.close()
        except BrokenPipeError:
            pass
        try:
            self.child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.child.terminate()
            self.child.wait(timeout=5)
        self.child.stdout.close()


def exercise(root, binaries, scratch, log):
    # END will write its attached document: every drawing here is a scratch copy.
    source = scratch / 'initial drawing.dwg'
    shutil.copyfile(root / 'crates/acad-cmd/tests/fixtures/hatch/HNETDEF.dwg', source)
    sock = scratch / 'gui.sock'
    fonts = [str(root / 'corpus/System')] if (root / 'corpus/System').is_dir() else []
    gui = subprocess.Popen([str(binaries / 'acad'), str(source), *fonts, '--api-socket', str(sock)],
                           stdout=log, stderr=log)
    mcp = None
    try:
        deadline = time.monotonic() + 15
        while not sock.exists():
            assert gui.poll() is None, 'GUI exited before creating its socket; see gui.log'
            assert time.monotonic() < deadline, 'GUI socket startup timed out'
            time.sleep(0.02)
        assert sock.stat().st_mode & 0o777 == 0o600
        mcp = Mcp(binaries / 'acad-mcp', ['--socket', sock], log)
        assert mcp.tool('state')['structuredContent'] == api(sock, 'state')
        mcp.tool('new')
        # Exercise shared selection through the attached native window, not
        # only an in-process Session: picks collect until Return, typed IDs
        # replace them, windows collect, and cancel adds no mutation.
        for value in ['POINT', '1,1', 'POINT', '3,3', 'POINT', '5,5']:
            mcp.tool('command', input=value)
        selection_geometry = api(sock, 'drawing')
        mcp.tool('command', input='LIST')
        for x, y in [(1, 1), (3, 3), (1, 1)]:
            mcp.tool('point', x=x, y=y)
        assert api(sock, 'state')['input'] == '1,2'
        assert api(sock, 'drawing') == selection_geometry
        mcp.tool('command', input='3')
        assert api(sock, 'state')['report'].startswith('3 POINT\n')
        mcp.tool('report', action='close')
        for value in ['LIST', 'W']:
            mcp.tool('command', input=value)
        mcp.tool('point', x=0, y=0)
        mcp.tool('point', x=4, y=4)
        assert api(sock, 'state')['input'] == '1,2'
        mcp.tool('command', input='')
        assert api(sock, 'state')['report'].startswith('1 POINT, 2 POINT\n')
        mcp.tool('report', action='close')
        mcp.tool('command', input='ERASE')
        mcp.tool('point', x=1, y=1)
        mcp.tool('cancel')
        assert api(sock, 'drawing') == selection_geometry
        mcp.tool('command', input='ERASE')
        for x, y in [(1, 1), (3, 3)]:
            mcp.tool('point', x=x, y=y)
        assert api(sock, 'state')['selectable_objects'] == 3
        mcp.tool('command', input='')
        assert api(sock, 'state')['selectable_objects'] == 1
        mcp.tool('command', input='UNDO')
        assert api(sock, 'drawing') == selection_geometry
        mcp.tool('new')
        for value in ['TEXT', 'C', '2,4', '1', '30', 'WIDE ',
                      'TEXT', 'R', '10,4', '1', '90', 'SHORT',
                      'TEXT', 'A', '0,0', '@4,0', 'AB', '', 'I']:
            mcp.tool('command', input=value)
        assert api(sock, 'state')['entities'] == 4
        text_geometry = api(sock, 'drawing')
        assert 'WIDE \r\n' in text_geometry['data']
        assert text_geometry['data'].count('TEXT,') == 4
        text_baseline = scratch / 'text baseline.dwg'
        mcp.tool('save', path=str(text_baseline))
        for value in ['CHANGE', 'LAST', '3,2', '45']:
            mcp.tool('command', input=value)
        assert api(sock, 'drawing') == text_geometry and not api(sock, 'state')['dirty']
        mcp.tool('cancel')
        assert api(sock, 'drawing') == text_geometry and not api(sock, 'state')['dirty']
        for value in ['CHANGE', 'LAST', '3,2', '45', 'DONE ']:
            mcp.tool('command', input=value)
        assert 'DONE \r\n' in api(sock, 'drawing')['data'] and api(sock, 'state')['dirty']
        mcp.tool('command', input='UNDO')
        assert api(sock, 'drawing') == text_geometry and not api(sock, 'state')['dirty']
        for value in ['TEXT', 'C', '4,6', '0,0', '@0,1', '5,6', 'POINT HEIGHT']:
            mcp.tool('command', input=value)
        baked_text = api(sock, 'drawing')
        for path in [scratch / 'text layout.dwg', scratch / 'text layout.DXF']:
            mcp.tool('save', path=str(path))
            reopened_text = Mcp(binaries / 'acad-mcp', ['--drawing', path], log)
            try:
                assert reopened_text.tool('state')['structuredContent']['entities'] == 5
                assert reopened_text.tool('drawing')['structuredContent'] == baked_text
            finally:
                reopened_text.close()
        text_png = mcp.tool('frame', width=800, height=600)
        (scratch / 'text-layout.png').write_bytes(
            base64.b64decode(text_png['content'][0]['data'], validate=True))
        mcp.tool('new')
        for value in ['LINE', '0,0', '6,0', '', 'LINE', '0,0', '0,6', '',
                      'FILLET', 'R', '2', 'FILLET', '1,2']:
            mcp.tool('command', input=value)
        assert api(sock, 'state')['fillet_radius'] == 2
        assert api(sock, 'state')['entities'] == 3
        fillet_path = scratch / 'fillet radius.dwg'
        mcp.tool('save', path=str(fillet_path))
        fillet_state = api(sock, 'state')
        assert not fillet_state['dirty']
        fresh_fillet = Mcp(binaries / 'acad-mcp', ['--drawing', fillet_path], log)
        try:
            state = fresh_fillet.tool('state')['structuredContent']
            assert state['fillet_radius'] == 2 and state['entities'] == 3 and not state['dirty']
            assert not fresh_fillet.tool('frame')['structuredContent']['diagnostics']
        finally:
            fresh_fillet.close()
        guarded = scratch / 'fillet guard.DXF'
        guarded.write_bytes(b'previous destination bytes')
        assert 'fillet' in str(api(sock, 'save', path=str(guarded), expect_error=True)).lower()
        assert guarded.read_bytes() == b'previous destination bytes'
        missing_fillet = scratch / 'fillet missing.DXF'
        api(sock, 'save', path=str(missing_fillet), expect_error=True)
        assert not missing_fillet.exists()
        api(sock, 'drawing', expect_error=True)
        state = api(sock, 'state')
        assert state['path'] == fillet_state['path'] and not state['dirty']
        for value in ['FILLET', 'R', '0']:
            mcp.tool('command', input=value)
        assert api(sock, 'drawing')['data'].count('ARC,') == 1
        mcp.tool('command', input='UNDO')
        assert api(sock, 'state')['fillet_radius'] == 2 and not api(sock, 'state')['dirty']
        fillet_png = mcp.tool('frame', width=800, height=600)
        (scratch / 'fillet.png').write_bytes(
            base64.b64decode(fillet_png['content'][0]['data'], validate=True))
        mcp.tool('new')
        for value in ['LINE', '1,0', '5,0', '', 'LINE', '0,1', '0,5', '']:
            mcp.tool('command', input=value)
        zero_before = api(sock, 'drawing')
        for value in ['FILLET', '1,2']:
            mcp.tool('command', input=value)
        zero_after = api(sock, 'drawing')
        assert zero_after != zero_before and zero_after['data'].count('LINE,') == 2
        assert zero_after['data'].count('ARC,') == 0
        for path in [scratch / 'zero fillet.dwg', scratch / 'zero fillet.DXF']:
            mcp.tool('save', path=str(path))
            fresh_zero = Mcp(binaries / 'acad-mcp', ['--drawing', path], log)
            try:
                assert fresh_zero.tool('drawing')['structuredContent'] == zero_after
            finally:
                fresh_zero.close()
        mcp.tool('command', input='UNDO')
        assert api(sock, 'drawing') == zero_before
        for value in ['ZOOM', 'W', '0,0', '20,2']:
            mcp.tool('command', input=value)
        window_view = api(sock, 'state')['view']
        assert window_view['center'] == dict(x=10, y=1) and window_view['height'] >= 2
        for value in ['ZOOM', '2X']:
            mcp.tool('command', input=value)
        relative_view = api(sock, 'state')['view']
        assert relative_view['center'] == window_view['center']
        assert relative_view['height'] == window_view['height'] / 2
        for value in ['ZOOM', 'P', 'PAN', '@2,3', '']:
            mcp.tool('command', input=value)
        assert api(sock, 'state')['view']['center'] == dict(x=8, y=-2)
        for value in ['ZOOM', 'P', 'PAN', '1,1', '@2,3']:
            mcp.tool('command', input=value)
        assert api(sock, 'state')['view']['center'] == dict(x=8, y=-2)
        for value in ['ZOOM', '1']:
            mcp.tool('command', input=value)
        absolute_view = api(sock, 'state')['view']
        for value in ['ZOOM', 'A']:
            mcp.tool('command', input=value)
        assert api(sock, 'state')['view'] == absolute_view
        for value in ['ZOOM', 'L', '2,3', '4']:
            mcp.tool('command', input=value)
        lower_view = api(sock, 'state')['view']
        assert lower_view['height'] == 4 and lower_view['center']['y'] == 5
        assert lower_view['center']['x'] > 2
        for center, height in [('1e308,1e308', '1'), ('0,0', '1e-308')]:
            for value in ['ZOOM', 'C', center]:
                mcp.tool('command', input=value)
            api(sock, 'command', input=height, expect_error=True)
            assert api(sock, 'state')['view'] == lower_view
            mcp.tool('cancel')
        # B4: a live REPEAT with one erased ordinary member keeps it in DWG,
        # omits it from historical DXF, and refuses an ambiguous whole-owner save.
        partial = scratch / 'partial member.dwg'
        subprocess.run(['cargo', 'test', '-q', '-p', 'acad-dwg', '--test', 'erased_members',
                        'small_native_partial_member_fixture_for_gui'], cwd=root, check=True,
                       env={**os.environ, 'B4_GUI_FIXTURE_PATH': str(partial)},
                       stdout=log, stderr=log)
        mcp.tool('open', path=str(partial), directories=fonts)
        partial_geometry = api(sock, 'drawing')
        partial_state = api(sock, 'state')
        assert partial_state['selectable_objects'] == 1 and not partial_state['dirty']
        assert partial_geometry['data'].count('LINE,') == 1
        assert not mcp.tool('frame')['structuredContent']['diagnostics']
        for path in [scratch / 'partial resave.dwg', scratch / 'partial live.DXF']:
            mcp.tool('save', path=str(path))
            reopened_partial = Mcp(binaries / 'acad-mcp', ['--drawing', path], log)
            try:
                assert reopened_partial.tool('drawing')['structuredContent'] == partial_geometry
                assert reopened_partial.tool('state')['structuredContent'][
                    'selectable_objects'] == 1
            finally:
                reopened_partial.close()
        assert (scratch / 'partial resave.dwg').read_bytes() == partial.read_bytes()
        for value in ['ERASE', 'LAST']:
            mcp.tool('command', input=value)
        ambiguous = scratch / 'partial ambiguous.dwg'
        ambiguous.write_bytes(b'previous destination bytes')
        api(sock, 'save', path=str(ambiguous), expect_error=True)
        assert ambiguous.read_bytes() == b'previous destination bytes'
        mcp.tool('command', input='OOPS')
        # Editing refreshes EXTENTS; the restored records must match exactly.
        def without_extents(text):
            return re.sub(r'EXTENTS,1\r\n[^\r]*\r\n', '', text)
        assert (without_extents(api(sock, 'drawing')['data'])
                == without_extents(partial_geometry['data']))
        # B1: INSERT an external drawing as a block and star-exploded, with one UNDO each.
        mcp.tool('new')
        for value in ['LINE', '0,0', '3,0', '', 'LINE', '3,0', '3,2', '']:
            mcp.tool('command', input=value)
        part = scratch / 'extpart.dwg'
        mcp.tool('save', path=str(part))
        mcp.tool('new')
        for value in ['INSERT', str(part), '5,5', '2', '2', '0']:
            mcp.tool('command', input=value)
        inserted = api(sock, 'state')
        assert inserted['blocks'] == 1 and inserted['selectable_objects'] == 1
        for value in ['INSERT', '*' + str(part), '10,0']:
            mcp.tool('command', input=value)
        assert api(sock, 'state')['selectable_objects'] == 3
        external = api(sock, 'drawing')
        for path in [scratch / 'external insert.dwg', scratch / 'external insert.DXF']:
            mcp.tool('save', path=str(path))
            reopened_external = Mcp(binaries / 'acad-mcp', ['--drawing', path], log)
            try:
                assert reopened_external.tool('drawing')['structuredContent'] == external
            finally:
                reopened_external.close()
        mcp.tool('command', input='UNDO')
        assert api(sock, 'state')['selectable_objects'] == 1
        missing_part = api(sock, 'state')
        mcp.tool('command', input='INSERT')
        assert 'not found' in str(api(sock, 'command', input=str(scratch / 'absent.dwg'),
                                      expect_error=True))
        mcp.tool('cancel')
        after_missing = api(sock, 'state')
        assert after_missing['selectable_objects'] == 1
        assert after_missing['blocks'] == missing_part['blocks']
        # H1: concentric squares; O hatches only the outer ring, I fills the outer loop.
        hatch_counts = {}
        for style in ['N', 'O', 'I']:
            mcp.tool('new')
            for side in [8, 4, 2]:
                low, high = f'{-side},{-side}', f'{side},{side}'
                for value in ['LINE', low, f'{side},{-side}', high, f'{-side},{side}', low, '']:
                    mcp.tool('command', input=value)
            for value in ['HATCH', f'LINE,{style}', '1', '0', 'ALL']:
                mcp.tool('command', input=value)
            hatch_counts[style] = api(sock, 'drawing')['data'].count('LINE,')
        assert len(set(hatch_counts.values())) == 3, hatch_counts
        for path in [scratch / 'hatch styles.dwg', scratch / 'hatch styles.DXF']:
            mcp.tool('save', path=str(path))
            reopened_hatch = Mcp(binaries / 'acad-mcp', ['--drawing', path], log)
            try:
                assert reopened_hatch.tool('drawing')['structuredContent'] == api(sock, 'drawing')
            finally:
                reopened_hatch.close()
        # H2: a pattern file beside the open document, and U double hatching with a style.
        mcp.tool('new')
        for side in [8, 4, 2]:
            low, high = f'{-side},{-side}', f'{side},{side}'
            for value in ['LINE', low, f'{side},{-side}', high, f'{-side},{side}', low, '']:
                mcp.tool('command', input=value)
        islands = scratch / 'islands.dwg'
        mcp.tool('save', path=str(islands))
        mcp.tool('open', path=str(islands), directories=fonts)
        (scratch / 'RAILS.PAT').write_bytes(
            b'*RAILS,two rails\r\n0, 0,0, 0,.5\r\n90, .25,0, .5,1, 0,-.5\r\n\x1a')
        plain = api(sock, 'drawing')['data'].count('LINE,')
        for value in ['HATCH', 'rails,O', '1', '0', 'ALL']:
            mcp.tool('command', input=value)
        rails = api(sock, 'drawing')['data'].count('LINE,')
        assert rails > plain
        mcp.tool('command', input='UNDO')
        for value in ['HATCH', 'U,I', '30', '0.5', 'Y', 'ALL']:
            mcp.tool('command', input=value)
        user_hatch = api(sock, 'drawing')
        assert user_hatch['data'].count('LINE,') > plain
        for path in [scratch / 'user hatch.dwg', scratch / 'user hatch.DXF']:
            mcp.tool('save', path=str(path))
            reopened_user = Mcp(binaries / 'acad-mcp', ['--drawing', path], log)
            try:
                assert reopened_user.tool('drawing')['structuredContent'] == user_hatch
            finally:
                reopened_user.close()
        # B2: circular ARRAY filling 270 degrees at 90 gives 4 items; BREAK by picked points.
        mcp.tool('new')
        for value in ['LINE', '0,0', '2,0', '', 'ARRAY', 'L', 'C', '4,3', '90', '-270']:
            mcp.tool('command', input=value)
        assert api(sock, 'state')['selectable_objects'] == 4
        for value in ['BREAK', '0.5,0', '1.5,0']:
            mcp.tool('command', input=value)
        broken = api(sock, 'drawing')
        assert api(sock, 'state')['selectable_objects'] == 5
        for path in [scratch / 'array break.dwg', scratch / 'array break.DXF']:
            mcp.tool('save', path=str(path))
            reopened_break = Mcp(binaries / 'acad-mcp', ['--drawing', path], log)
            try:
                assert reopened_break.tool('drawing')['structuredContent'] == broken
            finally:
                reopened_break.close()
        mcp.tool('command', input='UNDO')
        assert api(sock, 'state')['selectable_objects'] == 4
        # F1: saving over an existing drawing keeps its previous bytes as .BAK.
        backed = scratch / 'backed.dwg'
        mcp.tool('save', path=str(backed))
        first_bytes = backed.read_bytes()
        assert not (scratch / 'backed.bak').exists()
        mcp.tool('command', input='ERASE')
        mcp.tool('command', input='LAST')
        mcp.tool('save', path=str(backed))
        assert (scratch / 'backed.bak').read_bytes() == first_bytes
        assert backed.read_bytes() != first_bytes
        assert not [p for p in scratch.iterdir() if p.name.startswith('.acad-save-')]
        # Q1: a DIM (line ends, text point, Return for the measured text)
        # survives DWG and DXF reopen and is one UNDO step.
        mcp.tool('new')
        for value in ['DIM', '0,0', '4,0', '2,1', '']:
            mcp.tool('command', input=value)
        dimensioned = api(sock, 'drawing')
        dim_state = api(sock, 'state')
        assert dim_state['prompt'] == 'Command' and dim_state['selectable_objects'] > 0, dim_state
        for path in [scratch / 'dimension.dwg', scratch / 'dimension.DXF']:
            mcp.tool('save', path=str(path))
            reopened_dim = Mcp(binaries / 'acad-mcp', ['--drawing', path], log)
            try:
                assert reopened_dim.tool('drawing')['structuredContent'] == dimensioned
            finally:
                reopened_dim.close()
        mcp.tool('command', input='UNDO')
        assert api(sock, 'state')['selectable_objects'] == 0
        # K1: mouse SKETCH keeps strokes temporary until X records them as one UNDO.
        mcp.tool('new')
        for value in ['SKETCH', '0.1']:
            mcp.tool('command', input=value)
        mcp.tool('motion', x=100, y=100, width=800, height=600)
        mcp.tool('click', x=100, y=100, width=800, height=600)
        for x in range(120, 301, 20):
            mcp.tool('motion', x=x, y=100, width=800, height=600)
        for y in range(120, 301, 20):
            mcp.tool('motion', x=300, y=y, width=800, height=600)
        mcp.tool('click', x=300, y=300, width=800, height=600)
        sketch_state = api(sock, 'state')
        assert sketch_state['sketch']['temporary_lines'] == 2, sketch_state['sketch']
        assert sketch_state['entities'] == 0 and not sketch_state['dirty']
        mcp.tool('command', input='X')
        recorded = api(sock, 'state')
        assert recorded['entities'] == 2 and recorded['dirty']
        mcp.tool('command', input='UNDO')
        assert api(sock, 'state')['entities'] == 0
        # X1: the GUI event loop itself finishes a script across a nonblocking DELAY.
        mcp.tool('new')
        script = scratch / 'gui.scr'
        script.write_text('LINE 0,0 4,0\n\nDELAY 200\nCIRCLE 2,2 1\n')
        mcp.tool('script', path=str(script))
        deadline = time.monotonic() + 10
        while api(sock, 'script_status')['state'] != 'idle':
            assert time.monotonic() < deadline, api(sock, 'script_status')
            assert api(sock, 'state')['prompt'] is not None
            time.sleep(0.02)
        assert api(sock, 'state')['selectable_objects'] == 2
        mcp.tool('new')
        for value in ['SNAP', '0.5', 'ORTHO', 'ON', 'LINE']:
            mcp.tool('command', input=value)
        mcp.tool('point', x=1.1, y=2.1)
        mcp.tool('point', x=5.1, y=3.0)
        for value in ['', 'CIRCLE', '6,5', 'D', '2', 'TEXT', '1,7', '0.5', '0',
                      'MCP Rust', 'GRID', '0']:
            mcp.tool('command', input=value)
        for mode, points in [('2P', [(1.1, 1.1), (5.1, 1.1)]),
                             ('3P', [(5.1, 3.1), (3.1, 5.1), (1.1, 3.1)])]:
            mcp.tool('command', input='CIRCLE')
            mcp.tool('command', input=mode)
            for x, y in points:
                mcp.tool('point', x=x, y=y)
        for value in ['SOLID', '8,1', '11,1', '8,4', '', '']:
            mcp.tool('command', input=value)
        # POINT/CIRCLE/SOLID do not replace the last LINE/ARC continuation.
        for value in ['ARC', '']:
            mcp.tool('command', input=value)
        mcp.tool('point', x=7.1, y=4.1)
        for value in ['LINE', '', '@0,2', '',
                      'ARC', '9,5', 'C', '9,7', 'A', '90']:
            mcp.tool('command', input=value)
        for value in ['REPEAT', 'POINT', '8,7', 'POINT', '10,7',
                      'ENDREP', '2', '2', '1', '1']:
            mcp.tool('command', input=value)
        state = api(sock, 'state')
        assert state['entities'] == 11 and state['selectable_objects'] == 10 and state['dirty']
        before_copy = api(sock, 'drawing')
        for value in ['COPY', '0,1', '', 'LAST']:
            mcp.tool('command', input=value)
        copied = api(sock, 'state')
        assert copied['entities'] == 13 and copied['selectable_objects'] == 11
        mcp.tool('command', input='UNDO')
        assert api(sock, 'drawing') == before_copy
        state = api(sock, 'state')
        assert state['entities'] == 11 and state['selectable_objects'] == 10 and state['dirty']
        assert state['grid'] == dict(on=True, spacing=0.0)
        cli = [str(root / 'tools/acad_api.py'), '--socket', str(sock)]
        assert json.loads(subprocess.check_output([sys.executable, *cli, 'state'], timeout=35)) == state
        paths = [scratch / 'saved drawing.dwg', scratch / 'saved drawing.DXF']
        for path in paths:
            mcp.tool('save', path=str(path))
            assert not api(sock, 'state')['dirty']
        # The 160-pixel menu occupies a whole 160-pixel frame. Unload it so
        # every requested size has a visible drawing canvas for this assertion.
        mcp.tool('command', input='MENU')
        mcp.tool('command', input='')
        visible = base64.b64decode(api(sock, 'frame', width=800, height=600,
                                       format='rgba')['data'], validate=True)
        saved_bytes = {path: path.read_bytes() for path in paths}
        attachment = api(sock, 'state')['path']
        mcp.tool('command', input='LAYER OFF 1')
        assert api(sock, 'state')['off_layers'] == [1]
        hidden = base64.b64decode(api(sock, 'frame', width=800, height=600,
                                      format='rgba')['data'], validate=True)
        assert visible[:800 * 556 * 4] != hidden[:800 * 556 * 4]
        hidden_png = mcp.tool('frame', width=800, height=600)
        (scratch / 'layers-off.png').write_bytes(
            base64.b64decode(hidden_png['content'][0]['data'], validate=True))
        off_color = api(sock, 'state')['layers']['1']
        off_dxf = api(sock, 'drawing')['data']
        assert off_dxf.split('LAYERC,1\r\n', 1)[1].splitlines()[0].split(',')[1] == str(-off_color)
        for path in paths:
            mcp.tool('save', path=str(path))
            assert not api(sock, 'state')['dirty']
            if path.suffix.lower() == '.dwg':
                assert int.from_bytes(path.read_bytes()[0xca:0xcc], 'little', signed=True) == -off_color
            reopened_off = Mcp(binaries / 'acad-mcp', ['--drawing', path], log)
            try:
                reopened_state = reopened_off.tool('state')['structuredContent']
                assert reopened_state['off_layers'] == [1]
                assert reopened_state['layers']['1'] == off_color and not reopened_state['dirty']
            finally:
                reopened_off.close()
        mcp.tool('command', input='UNDO')
        assert api(sock, 'state')['off_layers'] == [] and api(sock, 'state')['dirty']
        restored = base64.b64decode(api(sock, 'frame', width=800, height=600,
                                        format='rgba')['data'], validate=True)
        assert restored[:800 * 556 * 4] == visible[:800 * 556 * 4]
        for path in paths:
            mcp.tool('save', path=str(path))
        saved_bytes = {path: path.read_bytes() for path in paths}
        attachment = api(sock, 'state')['path']
        # Color zero has no signed negative representation. Failed saves must
        # still preserve files, attachment, OFF state and recovery via UNDO.
        for value in ['LAYER COLOR 0', 'LAYER OFF 1']:
            mcp.tool('command', input=value)
        assert api(sock, 'state')['off_layers'] == [1]
        assert 'layer' in str(api(sock, 'drawing', expect_error=True)).lower()
        for path in paths:
            assert 'layer' in str(api(sock, 'save', path=str(path), expect_error=True)).lower()
            assert path.read_bytes() == saved_bytes[path]
        missing = scratch / 'off-zero-must-not-save.dwg'
        api(sock, 'save', path=str(missing), expect_error=True)
        assert not missing.exists()
        api(sock, 'command', input='END', expect_error=True)
        assert gui.poll() is None and api(sock, 'state')['path'] == attachment
        assert all(path.read_bytes() == saved_bytes[path] for path in paths)
        mcp.tool('command', input='LAYER ?')
        assert 'OFF' in api(sock, 'state')['report']
        for _ in range(2):
            mcp.tool('command', input='UNDO')
        assert api(sock, 'state')['off_layers'] == [] and not api(sock, 'state')['dirty']
        flat_geometry = api(sock, 'drawing')
        for value in ['REPEAT', 'ARRAY', 'LAST', 'R', '1', '2', '0', '10',
                      'ENDREP', '1', '1', '0', '0']:
            mcp.tool('command', input=value)
        assert api(sock, 'state')['dirty']
        nested_geometry = api(sock, 'drawing')
        nested_state = api(sock, 'state')
        nested_paths = [scratch / 'nested repeat.dwg', scratch / 'nested repeat.DXF']
        for path in nested_paths:
            mcp.tool('save', path=str(path))
            assert not api(sock, 'state')['dirty']
            reopened_nested = Mcp(binaries / 'acad-mcp', ['--drawing', path], log)
            try:
                reopened_state = reopened_nested.tool('state')['structuredContent']
                assert reopened_state['selectable_objects'] == nested_state['selectable_objects']
                assert reopened_nested.tool('drawing')['structuredContent'] == nested_geometry
            finally:
                reopened_nested.close()
        # Native whole-owner ERASE uses negative lexical records in DWG;
        # historical DXF remains a live export. Both reopen without a live/selectable owner.
        mcp.tool('command', input='ERASE')
        mcp.tool('command', input='LAST')
        assert api(sock, 'state')['selectable_objects'] == nested_state['selectable_objects'] - 1
        for path in [scratch / 'erased repeat.dwg', scratch / 'erased repeat.DXF']:
            mcp.tool('save', path=str(path))
            reopened_erased = Mcp(binaries / 'acad-mcp', ['--drawing', path], log)
            try:
                state = reopened_erased.tool('state')['structuredContent']
                assert state['selectable_objects'] == nested_state['selectable_objects'] - 1
                assert not state['dirty']
            finally:
                reopened_erased.close()
        mcp.tool('command', input='OOPS')
        assert api(sock, 'drawing') == nested_geometry and api(sock, 'state')['dirty']
        mcp.tool('command', input='UNDO')
        assert not api(sock, 'state')['dirty']
        mcp.tool('command', input='UNDO')
        assert api(sock, 'drawing') == nested_geometry
        for value in ['BLOCK', 'GUI_REPEAT', '0,0', 'LAST']:
            mcp.tool('command', input=value)
        for value in ['INSERT', 'GUI_REPEAT', '0,0', '1', '1', '0']:
            mcp.tool('command', input=value)
        before_wblock = api(sock, 'drawing')
        source_state = api(sock, 'state')
        exported = scratch / 'named repeat block.dwg'
        for value in ['WBLOCK', str(exported), 'GUI_REPEAT']:
            mcp.tool('command', input=value)
        assert api(sock, 'drawing') == before_wblock
        assert api(sock, 'state')['path'] == source_state['path']
        assert api(sock, 'state')['dirty'] == source_state['dirty']
        # The existing export now gets the original's replace question after
        # the file name; a non-Y answer (here the block name) keeps the file.
        exported_bytes = exported.read_bytes()
        mcp.tool('command', input='WBLOCK')
        mcp.tool('command', input=str(exported))
        assert api(sock, 'state')['prompt'] == 'WBLOCK: A drawing with this name already exists. Replace it? <N>'
        mcp.tool('command', input='GUI_REPEAT')
        assert api(sock, 'state')['prompt'] == 'Command'
        assert 'kept existing' in api(sock, 'state')['status']
        assert exported.read_bytes() == exported_bytes
        for value in ['WBLOCK', str(exported), 'Y', 'GUI_REPEAT']:
            mcp.tool('command', input=value)
        assert api(sock, 'state')['status'].startswith('Wrote block drawing')
        assert api(sock, 'drawing') == before_wblock
        assert api(sock, 'state')['dirty'] == source_state['dirty']
        reopened_block = Mcp(binaries / 'acad-mcp', ['--drawing', exported], log)
        try:
            assert reopened_block.tool('state')['structuredContent']['selectable_objects'] == 1
            assert reopened_block.tool('drawing')['structuredContent']['data'].count('REPEAT,') == 2
        finally:
            reopened_block.close()
        for path in [scratch / 'repeat block source.dwg', scratch / 'repeat block source.DXF']:
            mcp.tool('save', path=str(path))
            reopened_group = Mcp(binaries / 'acad-mcp', ['--drawing', path], log)
            try:
                state = reopened_group.tool('state')['structuredContent']
                assert state['blocks'] == source_state['blocks']
                assert state['selectable_objects'] == source_state['selectable_objects']
                assert reopened_group.tool('drawing')['structuredContent'] == before_wblock
            finally:
                reopened_group.close()
        # Reopen the earlier flat drawing to continue the lifecycle baseline.
        mcp.tool('open', path=str(paths[-1]), directories=fonts)
        assert api(sock, 'drawing') == flat_geometry and not api(sock, 'state')['dirty']
        assert all(path.read_bytes() == saved_bytes[path] for path in paths)
        # OPEN initializes a new session menu; remove it again so even the
        # 160-pixel capture below has a drawing canvas.
        for value in ['MENU', '']:
            mcp.tool('command', input=value)
        for width, height in [(800, 600), (320, 240), (160, 100)]:
            raw = api(sock, 'frame', width=width, height=height, format='rgba')
            pixels = base64.b64decode(raw['data'], validate=True)
            assert len(pixels) == width * height * 4 and raw['stride'] == width * 4
            assert bytes([80, 80, 80, 255]) in pixels
            png = mcp.tool('frame', width=width, height=height)
            encoded = base64.b64decode(png['content'][0]['data'], validate=True)
            assert encoded.startswith(b'\x89PNG\r\n\x1a\n')
            (scratch / f'grid-{width}x{height}.png').write_bytes(encoded)
        subprocess.check_output([sys.executable, *cli, 'frame', '--params',
                                 json.dumps(dict(width=160, height=100, format='rgba')),
                                 '--output', str(scratch / 'cli-frame.rgba')], timeout=35)
        assert (scratch / 'cli-frame.rgba').stat().st_size == 64000
        for path in paths:
            mcp.tool('new')
            mcp.tool('open', path=str(path), directories=fonts)
            state = api(sock, 'state')
            assert state['entities'] == 11 and state['selectable_objects'] == 10 and not state['dirty']
            assert state['path'] == str(path) and state['grid']['spacing'] == 0
        geometry = api(sock, 'drawing')
        mcp.tool('command', input='STATUS')
        assert api(sock, 'state')['report_view']['visible']
        assert not api(sock, 'state')['dirty']
        for value in ['HELP', 'ZOOM']:
            mcp.tool('command', input=value)
        first = api(sock, 'frame', width=640, height=240)
        (scratch / 'help-first.png').write_bytes(base64.b64decode(first['data'], validate=True))
        mcp.tool('report', action='end', width=640, height=240)
        assert api(sock, 'state')['report_view']['anchor'] > 0
        tail = api(sock, 'frame', width=640, height=240)
        assert tail['data'] != first['data']
        (scratch / 'help-last.png').write_bytes(base64.b64decode(tail['data'], validate=True))
        narrow = mcp.tool('frame', width=160, height=100)
        (scratch / 'help-narrow.png').write_bytes(base64.b64decode(narrow['content'][0]['data'], validate=True))
        subprocess.check_output([sys.executable, *cli, 'report', '--params',
                                 json.dumps(dict(action='home', width=640, height=240))], timeout=35)
        assert api(sock, 'frame', width=640, height=240)['data'] == first['data']
        # Click NEXT then CLOSE in the rendered report footer.
        api(sock, 'click', x=300, y=180, width=640, height=240)
        assert api(sock, 'state')['report_view']['anchor'] > 0
        api(sock, 'click', x=620, y=180, width=640, height=240)
        assert not api(sock, 'state')['report_view']['visible']
        for value in ['LIST', 'DBLIST', 'FILES']:
            mcp.tool('command', input=value)
            if value == 'LIST':
                assert api(sock, 'state')['prompt'].startswith('LIST:')
                mcp.tool('command', input='ALL')
            assert api(sock, 'state')['report_view']['visible']
        prompt = api(sock, 'state')['prompt']
        mcp.tool('report', action='close')
        assert api(sock, 'state')['prompt'] == prompt
        mcp.tool('cancel')
        assert api(sock, 'drawing') == geometry and not api(sock, 'state')['dirty']
        before = paths[-1].read_bytes()
        mcp.tool('command', input='POINT')
        mcp.tool('point', x=8, y=3)
        mcp.tool('command', input='UNDO')
        assert not api(sock, 'state')['dirty']
        mcp.tool('command', input='POINT')
        mcp.tool('point', x=8, y=3)
        assert mcp.tool('quit')['structuredContent']['quit'] is False
        assert api(sock, 'state')['prompt'].startswith('QUIT:')
        mcp.tool('command', input='N')
        assert gui.poll() is None and paths[-1].read_bytes() == before
        assert mcp.tool('command', input='END')['structuredContent']['quit'] is True
        gui.wait(timeout=10)
        assert gui.returncode == 0 and not sock.exists()
        assert paths[-1].read_bytes() != before
    finally:
        try:
            if mcp:
                mcp.close()
        finally:
            if gui.poll() is None:
                gui.terminate()
                gui.wait(timeout=5)
                sock.unlink(missing_ok=True)
    reopened = Mcp(binaries / 'acad-mcp', ['--drawing', paths[-1]], log)
    try:
        state = reopened.tool('state')['structuredContent']
        assert state['entities'] == 12 and state['selectable_objects'] == 11 and not state['dirty']
        assert state['format'] == 'dxf'
        assert reopened.tool('quit', discard=True)['structuredContent']['quit'] is True
        (scratch / 'reopened-state.json').write_text(json.dumps(state, indent=2) + '\n')
    finally:
        reopened.close()


def exercise_main_menu(binaries, scratch, log):
    """No drawing on the command line: the window starts at the Main Menu."""
    sock = scratch / 'menu.sock'
    gui = subprocess.Popen([str(binaries / 'acad'), '--api-socket', str(sock)],
                           stdout=log, stderr=log)
    mcp = None
    try:
        deadline = time.monotonic() + 15
        while not sock.exists():
            assert gui.poll() is None, 'Main Menu GUI exited before creating its socket'
            assert time.monotonic() < deadline, 'Main Menu GUI socket startup timed out'
            time.sleep(0.02)
        mcp = Mcp(binaries / 'acad-mcp', ['--socket', sock], log)
        state = mcp.tool('state')['structuredContent']
        assert state['main_menu']['screen'] == 'selection' and state['prompt'] == 'Enter selection'
        assert len(state['main_menu']['tasks']) == 8 and state['returns_to_main_menu'] is True
        api(sock, 'frame', width=320, height=200)
        drawing = scratch / 'MENU.DWG'
        for value in ['1', str(scratch / 'MENU'), 'LINE', '0,0', '4,4', '']:
            mcp.tool('command', input=value)
        assert api(sock, 'state')['path'] == str(drawing)
        ended = mcp.tool('command', input='END')['structuredContent']
        assert ended['quit'] is False and ended['state']['main_menu']['screen'] == 'selection'
        assert drawing.exists() and gui.poll() is None
        for value in ['5', '']:
            mcp.tool('command', input=value)
        assert (scratch / 'MENU.DXF').read_bytes().startswith(b'EXTENTS,1')
        mcp.tool('command', input='')
        assert mcp.tool('command', input='0')['structuredContent']['quit'] is True
        gui.wait(timeout=10)
        assert gui.returncode == 0 and not sock.exists()
    finally:
        try:
            if mcp:
                mcp.close()
        finally:
            if gui.poll() is None:
                gui.terminate()
                gui.wait(timeout=5)
                sock.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', type=Path, help='Built acad/acad-mcp directory')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    binaries = (args.bin_dir or root / 'target/debug').resolve()
    scratch = Path(tempfile.mkdtemp(prefix='acad-gui-lifecycle-'))
    print(f'Artifacts: {scratch}', flush=True)
    with (scratch / 'gui.log').open('w') as log:
        exercise(root, binaries, scratch, log)
        exercise_main_menu(binaries, scratch, log)
    print('PASS: Main Menu launch/NEW/END/Make DXF/Exit, attached GUI/MCP/API, TEXT C/R/A/repeat/point-height/CHANGE, REPEAT selection/COPY/nested/ERASE/OOPS/BLOCK/WBLOCK save-reopen and replace question, FILLET/ZOOM/PAN, erased REPEAT members, external INSERT, HATCH N/O/I/U and pattern files, circular ARRAY/BREAK, DIM, SCRIPT/DELAY event loop, END/SAVE .BAK backups, mouse SKETCH, layer OFF persistence/zero-color safe errors, ARC/LINE continuation, CIRCLE/triangle, reports/paging, GRID PNG/RGBA, DWG/DXF, UNDO, QUIT, END and socket cleanup')


if __name__ == '__main__':
    main()
