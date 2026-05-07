import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  distDir: process.env.NEXT_DIST_DIR ?? ".next",
  typescript: {
    ignoreBuildErrors: true
  },
  webpack(config) {
    config.ignoreWarnings = [
      ...(config.ignoreWarnings ?? []),
      {
        message: /Critical dependency: the request of a dependency is an expression/,
        module: /sales-(automation-server-runtime|seed)\.ts/
      }
    ];
    return config;
  }
};

export default nextConfig;
