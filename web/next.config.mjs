/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "standalone",
  experimental: {
    // Server Actions call the Rust daemon over a unix socket; no external
    // origins are involved.
    serverActions: { allowedOrigins: ["localhost:3333"] },
  },
};

export default nextConfig;
