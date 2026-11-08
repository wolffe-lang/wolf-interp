#!/bin/bash
# s288: copy a lane file into the tree's evidence (adapted by copy from s285's
# sanitize.sh). The lane's paths become $LANE, other lane paths $LANES, the
# Linux host's name H2, home directories $HOME, and another lane's lock line
# loses its lane id.
#   sanitize.sh IN OUT
sed -e 's#/Users/[a-z]*/lanes/s288#$LANE#g' -e 's#/home/[a-z]*/lanes/s288#$LANE#g' \
    -e 's#$LANE#$LANE#g' -e 's#$LANES#$LANES#g' \
    -e 's#/home/[a-z]*/lanes/#$LANES/#g' -e 's#/Users/[a-z]*/lanes/#$LANES/#g' \
    -e 's#mac build lock held by [A-Za-z0-9_-]* pid [0-9]*#mac build lock held by another lane#' \
    -e 's#H2#H2#g' -e 's#/Users/[a-z]*/#$HOME/#g' -e 's#/home/[a-z]*/#$HOME/#g' "$1" > "$2"
