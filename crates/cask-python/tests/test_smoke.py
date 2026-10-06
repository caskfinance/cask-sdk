import socket
import subprocess
import time
from pathlib import Path

import pytest

import cask_sdk
from cask_sdk import Cask

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
    with Cask("test-key", base_url=base_url) as cask:
        cask.wait_until_ready(5)
        assert cask.check_bool(FREE, "sso") is False
        assert cask.check_bool(PRO, "sso") is True
        assert cask.check_numeric(FREE, "seats") == 3
        assert cask.check_numeric(PRO_UNLIMITED, "seats") == float("inf")
        assert cask.check_enum(PRO, "support_tier") == "priority"

        with pytest.raises(cask_sdk.CustomerNotFoundError):
            cask.check_bool("nope", "sso")
        with pytest.raises(cask_sdk.FeatureNotFoundError):
            cask.check_bool(FREE, "nope")
        with pytest.raises(cask_sdk.WrongTypeError):
            cask.check_bool(FREE, "seats")


def test_not_ready_before_load():
    cask = Cask("test-key", base_url="http://127.0.0.1:1")
    try:
        with pytest.raises(cask_sdk.NotReadyError):
            cask.check_bool(FREE, "sso")
        with pytest.raises(cask_sdk.CaskError):
            cask.wait_until_ready(0.2)
    finally:
        cask.close()


def test_bad_key(base_url):
    cask = Cask("wrong", base_url=base_url)
    try:
        with pytest.raises(cask_sdk.AuthError):
            cask.wait_until_ready(2)
    finally:
        cask.close()
