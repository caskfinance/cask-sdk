# Cask Django demo

Single page that runs `check_bool` / `check_numeric` / `check_enum`.

```
cargo run -p cask-mock-server -- --addr 127.0.0.1:8099   # from repo root
cd crates/cask-python
.venv/bin/pip install django
.venv/bin/python examples/django_demo/manage.py runserver
```

Set `CASK_BASE_URL` to prefill the base URL field.
