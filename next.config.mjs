/** @type {import('next').NextConfig} */
const isDevelopment = process.env.NODE_ENV === "development";

const nextConfig = {
  ...(isDevelopment ? {} : { output: "export", distDir: "dist" }),
  poweredByHeader: false,
};

export default nextConfig;
