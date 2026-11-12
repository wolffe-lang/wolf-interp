#!/usr/bin/env bash
# li-wait: the plant's head, its base and its one hunk, printed first.
set -u
git symbolic-ref --short HEAD
git log --oneline -2
git diff --stat HEAD^ HEAD
git diff HEAD^ HEAD
