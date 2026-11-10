#!/usr/bin/env bash
set -euo pipefail
python3 -I ci/local-gate-test.py
python3 ci/tiers-test.py
python3 ci/is80/dist-test.py
python3 ci/is81-variadic/anchors.py
