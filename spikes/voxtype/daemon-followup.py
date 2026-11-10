import json, subprocess, tempfile, time, os

def command(*args):
    result = subprocess.run(['voxtype', *args], capture_output=True, text=True)
    return result.returncode, result.stdout

def state():
    return json.loads(command('status', '--format', 'json')[1])['class']

result = {}
owned = False
with tempfile.TemporaryDirectory(prefix='zuruf-daemon-') as directory:
    try:
        result['initialState'] = state()
        if result['initialState'] != 'idle':
            result['skipped'] = 'busy_external'
        else:
            follow = subprocess.Popen(['voxtype', 'status', '--follow', '--format', 'json'], stdout=subprocess.PIPE, text=True)
            path = directory + '/text.txt'
            code, _ = command('record', 'start', '--file='+path, '--no-auto-submit', '--no-smart-auto-submit', '--no-osd')
            owned = code == 0
            while True:
                status = json.loads(follow.stdout.readline())
                if status['class'] == 'recording':
                    result['startConfirmed'] = True
                    break
            follow.terminate()
            follow.wait()
            time.sleep(2)
            start = time.monotonic()
            code, output = command('record', 'stop', '--wait', '--json', '--timeout', '30')
            owned = False
            result['stopCode'] = code
            result['stop'] = json.loads(output)
            result['finalizeSeconds'] = time.monotonic()-start
            if state() == 'idle':
                code, _ = command('record', 'start', '--file='+path, '--no-auto-submit', '--no-smart-auto-submit', '--no-osd')
                owned = code == 0
                result['gapMs'] = round((time.monotonic()-start)*1000)
    finally:
        if owned:
            for _ in range(100):
                if state() == 'recording':
                    command('record', 'cancel')
                    break
                time.sleep(0.02)
            for _ in range(100):
                if state() == 'idle':
                    break
                time.sleep(0.02)
        result['finalState'] = state()
        with open('spikes/voxtype/daemon-followup-evidence.json', 'w') as file:
            json.dump(result, file, indent=2)
        print(json.dumps(result, indent=2))
