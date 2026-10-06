import os
import threading

import cask
from cask import Client, Config
from django.shortcuts import render

CHECKS = {
    "bool": "check_bool",
    "numeric": "check_numeric",
    "enum": "check_enum",
}

_clients: dict[tuple[str, str], Client] = {}
_lock = threading.Lock()


def get_client(api_key: str, base_url: str) -> Client:
    """Reuse one client per (api_key, base_url) so the snapshot is fetched once."""
    key = (api_key, base_url)
    with _lock:
        client = _clients.get(key)
        if client is None:
            client = Client(Config(api_key, base_url=base_url or None))
            client.wait_until_ready(5)
            _clients[key] = client
        return client


def index(request):
    values = {
        "api_key": "test-key",
        "base_url": os.environ.get("CASK_BASE_URL", ""),
        "customer": "",
        "feature": "",
        "check": "bool",
    }
    result = None
    error = None

    if request.method == "POST":
        for name in values:
            values[name] = request.POST.get(name, values[name]).strip()
        try:
            if values["check"] not in CHECKS:
                raise ValueError(f"unknown check type: {values['check']}")
            if not values["customer"] or not values["feature"]:
                raise ValueError("customer and feature are required")
            client = get_client(values["api_key"], values["base_url"])
            result = repr(getattr(client, CHECKS[values["check"]])(values["customer"], values["feature"]))
        except (cask.CaskError, ValueError) as e:
            error = f"{type(e).__name__}: {e}"

    return render(
        request,
        "index.html",
        {"values": values, "checks": list(CHECKS), "result": result, "error": error},
    )
