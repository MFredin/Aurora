FROM node:22-slim AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY tsconfig.json ./
COPY src ./src
RUN npm run build && npm prune --omit=dev

FROM node:22-slim
ENV NODE_ENV=production
WORKDIR /app
COPY --from=build /app/node_modules ./node_modules
COPY --from=build /app/dist ./dist
COPY package.json ./
# Mount a persistent volume at /app/data (Railway: service > Volumes). Railway rejects the VOLUME keyword.
ENV DATABASE_PATH=/app/data/sync.db
# Runs as root because Railway mounts volumes root-owned. Switch to USER node on hosts that don't.
EXPOSE 3000
CMD ["node", "--disable-warning=ExperimentalWarning", "dist/index.js"]
