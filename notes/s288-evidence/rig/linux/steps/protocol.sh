#!/usr/bin/env bash
set -euo pipefail
cargo run -- protocol validate vendor/upstream/corpus/protocol/valid.json vendor/upstream/corpus/protocol/with-extensions.json

