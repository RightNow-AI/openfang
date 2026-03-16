module.exports = {
  apps: [{
    name: 'whatsapp-gateway',
    script: 'index.js',
    cwd: '/data/whatsapp-gateway',
    node_args: '--experimental-vm-modules',
    watch: false,
    autorestart: true,
    max_restarts: 50,
    min_uptime: '10s',
    restart_delay: 5000,
    max_memory_restart: '256M',
    exp_backoff_restart_delay: 1000,
    error_file: '/data/whatsapp-gateway/logs/pm2-error.log',
    out_file: '/data/whatsapp-gateway/logs/pm2-out.log',
    merge_logs: true,
    time: true,
    env: {
      NODE_ENV: 'production',
      OPENFANG_DEFAULT_AGENT: 'ambrogio',
    },
  }],
};
