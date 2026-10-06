import socket
import subprocess
import time
from pathlib import Path

import pytest

import cask
from cask import Client, Config

ROOT = Path(__file__).resolve().parents[3]
FREE = "customer_01A10971485672EFB40302EAE6021F01"
PRO = "customer_01A10971485672EFB40302EAE6021F02"
PRO_UNLIMITED = "customer_01A10971485672EFB40302EAE6021F03"


@pytest.fixture(scope="module")
def base_url():
    subprocess.run(
        ["cargo", "build", "-q", "-p", "cask-mock-server"], cwd=ROOT, check=True
    )
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
    proc = subprocess.Popen(
        [str(ROOT / "target/debug/cask-mock-server"), "--addr", f"127.0.0.1:{port}"],
        stderr=subprocess.DEVNULL,
    )
    for _ in range(50):
        try:
            socket.create_connection(("127.0.0.1", port), timeout=0.1).close()
            break
        except OSError:
            time.sleep(0.1)
    yield f"http://127.0.0.1:{port}"
    proc.terminate()
    proc.wait()


def test_checks(base_url):
    with Client(Config("test-key", base_url=base_url)) as client:
        client.wait_until_ready(5)
        assert client.check_bool(FREE, "sso") is False
        assert client.check_bool(PRO, "sso") is True
        assert client.check_numeric(FREE, "seats") == 3
        assert client.check_numeric(PRO_UNLIMITED, "seats") == float("inf")
        assert client.check_enum(PRO, "support_tier") == "priority"

        with pytest.raises(cask.CustomerNotFoundError):
            client.check_bool("nope", "sso")
        with pytest.raises(cask.FeatureNotFoundError):
            client.check_bool(FREE, "nope")
        with pytest.raises(cask.WrongTypeError):
            client.check_bool(FREE, "seats")


def test_not_ready_before_load():
    client = Client(Config("test-key", base_url="http://127.0.0.1:1"))
    try:
        with pytest.raises(cask.NotReadyError):
            client.check_bool(FREE, "sso")
        with pytest.raises(cask.CaskError):
            client.wait_until_ready(0.2)
    finally:
        client.close()


def test_bad_key(base_url):
    client = Client(Config("wrong", base_url=base_url))
    try:
        with pytest.raises(cask.AuthError):
            client.wait_until_ready(2)
    finally:
        client.close()
