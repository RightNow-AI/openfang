#!/bin/bash
# Drop root privileges and run openfang as the openfang user
chown -R openfang:openfang /data 2>/dev/null
chown -R openfang:openfang /home/openfang 2>/dev/null
exec gosu openfang openfang "$@"
