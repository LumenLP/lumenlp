import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  output: "export",
  images: { unoptimized: true },
  transpilePackages: ["@creit.tech/stellar-wallets-kit"],
  webpack(config) {
    config.resolve.alias["sodium-native"] = false;
    return config;
  },
};

export default nextConfig;
