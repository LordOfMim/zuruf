"""Ambient pipeline test; never downloads or modifies Voxtype configuration."""
import json, resource, subprocess, tempfile, time, wave
with tempfile.TemporaryDirectory(prefix='zuruf-local-audio-') as directory:
    path = directory + '/ambient.wav'
    child = subprocess.Popen(['pw-record', '--raw', '--rate', '16000', '--channels', '1', '--format', 's16', '-'], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    try:
        raw = child.stdout.read(96000)
    finally:
        child.terminate()
        child.wait()
    with wave.open(path, 'wb') as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(16000)
        wav.writeframes(raw)
    results = []
    for _ in range(2):
        start = time.monotonic()
        output = subprocess.run(['voxtype', '--quiet', '--whisper-mode', 'local', '--model', 'base.en', '--language', 'en', '--engine', 'whisper', 'transcribe', path], capture_output=True, text=True)
        results.append(dict(seconds=round(time.monotonic()-start, 3), code=output.returncode, stdout=output.stdout, stderr=output.stderr[-500:], peakChildRssKiB=resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss))
    print(json.dumps(results, indent=2))
    with open('spikes/voxtype/transcribe-local-evidence.json', 'w') as file:
        json.dump(results, file, indent=2)
