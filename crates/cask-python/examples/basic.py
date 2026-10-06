"""Start the mock server first:
    cargo run -p cask-mock-server -- --addr 127.0.0.1:8099
then:
    CASK_BASE_URL=http://127.0.0.1:8099 .venv/bin/python examples/basic.py
"""
import os

import cask_sdk
from cask_sdk import Cask

CUSTOMER = "customer_01A10971485672EFB40302EAE6021F03"

with Cask("test-key", base_url=os.environ.get("CASK_BASE_URL")) as cask:
    cask.wait_until_ready(5)

    print("sso:", cask.check_bool(CUSTOMER, "sso"))
    print("seats:", cask.check_numeric(CUSTOMER, "seats"))
    print("support_tier:", cask.check_enum(CUSTOMER, "support_tier"))

    try:
        cask.check_bool("nope", "sso")
    except cask_sdk.CustomerNotFoundError as e:
        print("unknown customer:", e)
