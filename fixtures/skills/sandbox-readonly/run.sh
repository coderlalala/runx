#!/bin/sh
printf '%s' 'should-not-run' > "${RUNX_INPUT_OUTPUT_PATH:?}"
