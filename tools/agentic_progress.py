#!/usr/bin/env python3
"""Validate and update the native implementation loop ledger (local, no services)."""
import argparse
import datetime
import json
import os
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parent.parent
LEDGER = ROOT / 'docs/superpowers/plans/2026-10-04-agentic-progress.json'
STATES = {'pending', 'ready', 'running', 'review', 'fixing', 'verified', 'blocked', 'deferred'}
TRANSITIONS = {
    'pending': {'ready', 'deferred', 'blocked'},
    'ready': {'running', 'blocked', 'deferred'},
    'running': {'review', 'blocked'},
    'review': {'fixing', 'verified', 'blocked'},
    'fixing': {'review', 'blocked'},
    'blocked': {'ready', 'fixing', 'deferred'},
    'deferred': {'ready'},
    'verified': set(),
}


def validate(data):
    tasks = data['tasks']
    if len({t['id'] for t in tasks}) != len(tasks):
        raise ValueError('duplicate task IDs')
    by_id = {t['id']: t for t in tasks}
    visiting, visited = set(), set()

    def visit(task_id):
        if task_id in visiting:
            raise ValueError(f'dependency cycle at {task_id}')
        if task_id in visited:
            return
        if task_id not in by_id:
            raise ValueError(f'unknown dependency: {task_id}')
        task = by_id[task_id]
        if task['status'] not in STATES:
            raise ValueError(f'invalid state for {task_id}')
        if task['status'] == 'verified':
            missing = [name for name in task['required_checks']
                       if task['checks'].get(name, {}).get('result') != 'passed']
            if missing or task.get('review') != 'pass':
                raise ValueError(f'{task_id} verification lacks checks/review: {missing}')
            if task['kind'] == 'implementation' and not task.get('integrated'):
                raise ValueError(f'{task_id} is not integrated')
        visiting.add(task_id)
        for dependency in task['depends_on']:
            visit(dependency)
            if task['status'] in {'ready', 'running', 'review', 'fixing', 'verified'}:
                if by_id[dependency]['status'] != 'verified':
                    raise ValueError(f'{task_id} dependency {dependency} is not verified')
        visiting.remove(task_id)
        visited.add(task_id)

    for task in tasks:
        visit(task['id'])
    active = sum(t['status'] in {'running', 'fixing'} for t in tasks)
    if active > data['worker_limit']:
        raise ValueError('worker concurrency limit exceeded')


def save(path, data):
    # Replace only after a complete validated ledger has been written.
    validate(data)
    with tempfile.NamedTemporaryFile(mode='w', dir=path.parent,
                                     prefix=f'.{path.name}.', delete=False) as out:
        temporary = Path(out.name)
        json.dump(data, out, indent=2, ensure_ascii=False)
        out.write('\n')
        out.flush()
        os.fsync(out.fileno())
    try:
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ledger', type=Path, default=LEDGER)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('status')
    commands.add_parser('validate')
    update = commands.add_parser('update')
    update.add_argument('task')
    update.add_argument('--status', choices=sorted(STATES))
    update.add_argument('--note', required=True)
    update.add_argument('--agent')
    update.add_argument('--artifact', action='append', default=[])
    update.add_argument('--review', choices=['pass', 'changes_requested'])
    update.add_argument('--integrated', action='store_true')
    update.add_argument('--check', action='append', default=[],
                        help='name=passed|failed|not_run; details go in --note/artifacts')
    args = parser.parse_args()
    data = json.loads(args.ledger.read_text())
    validate(data)
    if args.command == 'validate':
        print('Ledger valid; dependencies and completion gates checked.')
        return
    if args.command == 'status':
        for task in data['tasks']:
            print(f"{task['id']:4} {task['status']:9} {task['objective']}")
        return
    task = next((t for t in data['tasks'] if t['id'] == args.task), None)
    if task is None:
        parser.error(f'unknown task: {args.task}')
    previous = task['status']
    if args.status and args.status != previous:
        if args.status not in TRANSITIONS[previous]:
            parser.error(f'invalid transition: {previous} -> {args.status}')
        task['status'] = args.status
        if args.status == 'fixing':
            if task['fix_attempts'] >= data['max_fix_attempts']:
                parser.error('repair budget exhausted; record blocker and re-scope')
            task['fix_attempts'] += 1
            # Old approvals/checks do not approve revised implementation.
            task['review'] = None
            task['integrated'] = False
            task['checks'] = {}
    timestamp = datetime.datetime.now(datetime.timezone.utc).isoformat()
    if args.agent:
        task['agent'] = args.agent
    if args.review:
        task['review'] = args.review
    if args.integrated:
        task['integrated'] = True
    for check in args.check:
        name, separator, result = check.partition('=')
        if not separator or not name or result not in {'passed', 'failed', 'not_run'}:
            parser.error(f'invalid check: {check}')
        task['checks'][name] = {'result': result, 'at': timestamp, 'note': args.note}
    task['artifacts'].extend(p for p in args.artifact if p not in task['artifacts'])
    task['latest_note'] = args.note
    task['updated_at'] = timestamp
    data['updated_at'] = timestamp
    data['events'].append({'at': timestamp, 'task': task['id'], 'from': previous,
                           'to': task['status'], 'note': args.note,
                           'checks': args.check, 'review': args.review})
    try:
        save(args.ledger, data)
    except ValueError as error:
        parser.error(str(error))
    print(f"{task['id']}: {previous} -> {task['status']}")


if __name__ == '__main__':
    main()
