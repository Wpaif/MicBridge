"""Run the native-input regression in an isolated rootful Xwayland server."""
import os
from pathlib import Path
import select
import subprocess
import sys

read_fd, write_fd = os.pipe()
server = subprocess.Popen(
    ["Xwayland", "-displayfd", str(write_fd), "-geometry", "800x600", "-nolisten", "tcp", "-ac", "-noreset"],
    pass_fds=(write_fd,), stdout=subprocess.DEVNULL,
)
os.close(write_fd)
try:
    assert select.select([read_fd], [], [], 10)[0], "Xwayland startup timed out"
    number = os.read(read_fd, 32).decode().strip()
    assert number.isdigit(), "Xwayland failed to start"
    env = dict(os.environ, DISPLAY=f":{number}", GDK_BACKEND="x11", G_DEBUG="fatal-warnings")
    root = Path(__file__).resolve().parents[2]
    result = subprocess.run(
        ["cargo", "test", "--offline", "--manifest-path", str(root / "app/Cargo.toml"),
         "slider_wheel_scrolls", "--", "--ignored", "--test-threads=1", "--nocapture"],
        cwd=root, env=env,
    )
finally:
    os.close(read_fd)
    server.terminate()
    try:
        server.wait(timeout=5)
    except subprocess.TimeoutExpired:
        server.kill()
        server.wait()
sys.exit(result.returncode)
