# Rustrest Cloud

Rustrest supports self-hosted cloud team synchronization.

## Setup using Docker

Rustrest cloud image can be pulled from GitHub Container Registry:

```bash
docker pull ghcr.io/sojebsikder/rustrest-cloud:latest
```

We can run migrate db using this command:

```bash
sudo docker run --rm --add-host=host.docker.internal:host-gateway --env-file .env -p 4001:4000 ghcr.io/sojebsikder/rustrest-cloud:latest migrate up
```

And run using this command:

```bash
sudo docker run -d --add-host=host.docker.internal:host-gateway --env-file .env -p 4001:4000 ghcr.io/sojebsikder/rustrest-cloud:latest server
```

Note: ensure .env file is present in the current directory.

.env file content:

```bash
APP_NAME=go-boilerplate
APP_URL=http://localhost:4000
PORT=4000

JWT_SECRET=secret123
JWT_EXPIRY=86400000

DATABASE_URL=postgres://postgres:root@postgres:5432/testdemo?sslmode=disable
REDIS_URL="rediss://user:password@host:6379"

# general per-IP limit across the API (/health and /metrics are exempt)
RATE_LIMIT_MAX_REQUESTS=300
RATE_LIMIT_DURATION=1m
# login attempts per IP+email (an IP gets 5x across all emails), and registrations per IP
AUTH_RATE_LIMIT_MAX_ATTEMPTS=10
AUTH_RATE_LIMIT_DURATION=15m
# Comma-separated proxy IPs/CIDRs allowed to set X-Forwarded-For (e.g. nginx). Empty = trust none.
TRUSTED_PROXIES=

ACCESS_TOKEN_TTL=1h
REFRESH_TOKEN_TTL=720h
REFRESH_REUSE_GRACE=30s
```
