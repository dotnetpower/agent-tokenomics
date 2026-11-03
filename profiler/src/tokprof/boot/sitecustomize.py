"""Loaded automatically by Python when this directory is on PYTHONPATH (set by `tokprof run`).

It first runs any other sitecustomize module the interpreter would have loaded, then installs the tokprof hook.
Everything is wrapped so a failure here can never stop the application from starting.
"""

import importlib.util
import os
import sys

_here = os.path.dirname(os.path.abspath(__file__))


def _run_chained():
    for entry in sys.path:
        try:
            if os.path.abspath(entry or ".") == _here:
                continue
            candidate = os.path.join(entry, "sitecustomize.py")
            if os.path.isfile(candidate):
                spec = importlib.util.spec_from_file_location("_chained_sitecustomize", candidate)
                module = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(module)
                return
        except Exception:
            return


try:
    _run_chained()
except Exception:
    pass

try:
    sys.path.insert(0, _here)
    import _tokprof_hook

    _tokprof_hook.install()
except Exception:
    pass
finally:
    try:
        sys.path.remove(_here)
    except ValueError:
        pass
