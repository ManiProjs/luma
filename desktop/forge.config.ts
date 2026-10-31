import type { ForgeConfig } from "@electron-forge/shared-types";
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { MakerSquirrel } from "@electron-forge/maker-squirrel";
import { MakerZIP } from "@electron-forge/maker-zip";
import { MakerDeb } from "@electron-forge/maker-deb";
import { MakerRpm } from "@electron-forge/maker-rpm";
import { VitePlugin } from "@electron-forge/plugin-vite";
import { FusesPlugin } from "@electron-forge/plugin-fuses";
import { FuseV1Options, FuseVersion } from "@electron/fuses";

const repositoryRoot = path.resolve(process.cwd(), "..");
const coreBinaryName = process.platform === "win32" ? "luma-core.exe" : "luma-core";
const coreBinaryPath = path.join(repositoryRoot, "target", "release", coreBinaryName);

const config: ForgeConfig = {
  packagerConfig: {
    asar: true,
    extraResource: [coreBinaryPath],
  },
  hooks: {
    prePackage: async (_forgeConfig, platform, arch) => {
      if (platform !== process.platform || arch !== process.arch) {
        throw new Error(
          `The bundled Luma core is built for ${process.platform}/${process.arch}; ` +
          `package the desktop app for that same target or cross-compile the Rust core first.`,
        );
      }
      execFileSync("cargo", ["build", "--release", "--no-default-features"], {
        cwd: repositoryRoot,
        stdio: "inherit",
      });
    },
    postPackage: async (_forgeConfig, { outputPaths, platform }) => {
      if (outputPaths.length === 0) {
        throw new Error("Electron Forge completed without producing a packaged application.");
      }
      for (const outputPath of outputPaths) {
        const resourcesPath = platform === "darwin"
          ? path.join(outputPath, "Contents", "Resources")
          : path.join(outputPath, "resources");
        const bundledCore = path.join(resourcesPath, coreBinaryName);
        if (!existsSync(bundledCore)) {
          throw new Error(`Packaged Luma core is missing from ${resourcesPath}.`);
        }
      }
      console.log(`Verified bundled Luma core in ${outputPaths.length} packaged app(s).`);
    },
  },
  rebuildConfig: {},
  makers: [
    new MakerSquirrel({}),
    new MakerZIP({}, ["darwin"]),
    new MakerRpm({}),
    new MakerDeb({}),
  ],
  plugins: [
    new VitePlugin({
      // `build` can specify multiple entry builds, which can be Main process, Preload scripts, Worker process, etc.
      // If you are familiar with Vite configuration, it will look really familiar.
      build: [
        {
          // `entry` is just an alias for `build.lib.entry` in the corresponding file of `config`.
          entry: "src/main.ts",
          config: "vite.main.config.ts",
          target: "main",
        },
        {
          entry: "src/preload.ts",
          config: "vite.preload.config.ts",
          target: "preload",
        },
      ],
      renderer: [
        {
          name: "main_window",
          config: "vite.renderer.config.mts",
        },
      ],
    }),
    // Fuses are used to enable/disable various Electron functionality
    // at package time, before code signing the application
    new FusesPlugin({
      version: FuseVersion.V1,
      [FuseV1Options.RunAsNode]: false,
      [FuseV1Options.EnableCookieEncryption]: true,
      [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: false,
      [FuseV1Options.EnableNodeCliInspectArguments]: false,
      [FuseV1Options.EnableEmbeddedAsarIntegrityValidation]: true,
      [FuseV1Options.OnlyLoadAppFromAsar]: true,
    }),
  ],
};

export default config;
