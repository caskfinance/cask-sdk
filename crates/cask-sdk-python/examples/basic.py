"""Start the mock server first:
    cargo run -p cask-mock-server -- --addr 127.0.0.1:8099
then:
    CASK_BASE_URL=http://127.0.0.1:8099 .venv/bin/python examples/basic.py
"""
import os

import cask
from cask import Client, Config

CUSTOMER = "customer_01A10971485672EFB40302EAE6021F03"

with Client(Config("test-key", base_url=os.environ.get("CASK_BASE_URL"))) as client:
    client.wait_until_ready(5)

    print("sso:", client.check_bool(CUSTOMER, "sso"))
    print("seats:", client.check_numeric(CUSTOMER, "seats"))
    print("support_tier:", client.check_enum(CUSTOMER, "support_tier"))

    try:
        client.check_bool("nope", "sso")
    except cask.CustomerNotFoundError as e:
        print("unknown customer:", e)
