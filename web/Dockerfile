# syntax=docker/dockerfile:1.7

# ─── Stage 1: deps ──────────────────────────────────────────────────
# Just package.json + bun.lock → cache-friendly deps layer.
FROM oven/bun:1.2-alpine AS deps
WORKDIR /app
COPY package.json bun.lock* ./
RUN bun install --frozen-lockfile

# ─── Stage 2: build ─────────────────────────────────────────────────
# Compile Next.js to standalone output. `next build` uses Node under the
# hood; that's fine — our runtime is bun, and bun:sqlite is marked
# external in next.config.mjs, so Node never tries to resolve it.
FROM oven/bun:1.2-alpine AS builder
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .
ENV NEXT_TELEMETRY_DISABLED=1
RUN bun run build

# ─── Stage 3: runtime ──────────────────────────────────────────────
# Small image, runs under bun. The DB and socket come from bind mounts
# under /data; see docker-compose.yml / `worklog web up`.
FROM oven/bun:1.2-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    PORT=3000 \
    HOSTNAME=0.0.0.0 \
    WORKLOG_DB=/data/worklog.db \
    WORKLOG_SOCKET=/data/api.sock

# Copy the standalone bundle + static assets. Standalone mode produces
# a ./server.js that includes a minimal node_modules tree so image
# size stays small.
COPY --from=builder /app/.next/standalone ./
COPY --from=builder /app/.next/static ./.next/static
COPY --from=builder /app/public ./public

# The app reads bun:sqlite at runtime; make sure bun is on PATH
# (it already is on the base image, but be explicit so ENTRYPOINT
# calls don't accidentally shell out to node).
EXPOSE 3000
CMD ["bun", "run", "server.js"]
