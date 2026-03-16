#!/bin/bash
# Drop root privileges and run openfang as the openfang user
chown -R openfang:openfang /data 2>/dev/null
chown -R openfang:openfang /home/openfang 2>/dev/null

# Resurrect PM2 processes (whatsapp-gateway etc.) before starting OpenFang
gosu openfang bash -c 'pm2 resurrect 2>/dev/null || true'

exec gosu openfang openfang "$@"
