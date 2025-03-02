"""Configure pytest to automatically build Rust extensions."""

from __future__ import annotations

import os
import subprocess
import time
from typing import Any

import pytest


def pytest_configure(config: Any) -> None:
    """Build Rust extension before running tests.

    This hook runs once before any tests are collected.
    """
    if os.environ.get("SKIP_RUST_BUILD"):
        return

    print("\n🔧 Building Rust extension with maturin...")
    start_time = time.time()

    try:
        # Use universal_newlines=True (text mode) with utf-8 encoding
        process = subprocess.Popen(
            ["maturin", "develop", "--release"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            universal_newlines=True,
            encoding="utf-8",  # Explicitly set UTF-8 encoding
            errors="replace",  # Replace invalid chars instead of failing
        )

        _stdout, stderr = process.communicate()

        if process.returncode != 0:
            pytest.exit(f"❌ Build failed:\n{stderr}")

    except Exception as e:
        pytest.exit(f"❌ Build process error: {str(e)}")

    duration = time.time() - start_time
    print(f"✅ Build successful (took {duration:.2f}s)")
