from __future__ import annotations

import sys
from pathlib import Path

_BACKEND_DIR = Path(__file__).resolve().parent
_backend_path = str(_BACKEND_DIR)
if _backend_path not in sys.path:
    sys.path.insert(0, _backend_path)
