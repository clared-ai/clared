# Clared Python client

Python client for the Clared execution-envelope reference implementation.

```bash
pip install -e .
```

The client opens authenticated sessions and routes explicit calls through `ClaredSession.call_tool`. It does not sandbox an existing framework automatically. A hard boundary requires withholding downstream credentials and removing alternate egress paths.

See the repository [README](../README.md#python-integration) and [fault-injection demo](../examples/fault_injection_demo.py).
