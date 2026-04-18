/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "standalone",
  experimental: {
    // Server Actions call the Rust daemon over a unix socket; no external
    // origins are involved.
    serverActions: { allowedOrigins: ["localhost:3333"] },
  },
  serverExternalPackages: ["better-sqlite3"],
  // bun:sqlite is a Bun runtime builtin, not an npm package; webpack
  // doesn't know the `bun:` URI scheme. Mark it external so the generated
  // server bundle just keeps `require("bun:sqlite")` verbatim — bun
  // resolves it at runtime.
  webpack: (config, { isServer }) => {
    if (isServer) {
      const externals = Array.isArray(config.externals)
        ? config.externals
        : [config.externals].filter(Boolean);
      externals.push({ "bun:sqlite": "commonjs bun:sqlite" });
      config.externals = externals;
    }
    return config;
  },
};

export default nextConfig;
