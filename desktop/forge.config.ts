import type { ForgeConfig } from "@electron-forge/shared-types";
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import path from "node:path";
import { MakerSquirrel } from "@electron-forge/maker-squirrel";
import { MakerZIP } from "@electron-forge/maker-zip";
import { MakerDeb } from "@electron-forge/maker-deb";
import { MakerRpm } from "@electron-forge/maker-rpm";
import { AutoUnpackNativesPlugin } from "@electron-forge/plugin-auto-unpack-natives";
import { VitePlugin } from "@electron-forge/plugin-vite";
import { FusesPlugin } from "@electron-forge/plugin-fuses";
import { FuseV1Options, FuseVersion } from "@electron/fuses";

const repositoryRoot = path.resolve(process.cwd(), "..");
const coreBinaryName = process.platform === "win32" ? "luma-core.exe" : "luma-core";
const coreBinaryPath = path.join(repositoryRoot, "target", "release", coreBinaryName);
const assetsPath = path.join(__dirname, "assets");
// Use the same complete Luma mark on every platform. Electron Packager
// resolves this extensionless base to .icns (macOS) or .ico (Windows);
// Linux uses the PNG supplied to BrowserWindow in main.ts.
const appIconPath = path.join(assetsPath, "icon");

const config: ForgeConfig = {
  packagerConfig: {
    asar: true,
    // Electron Packager selects the platform-specific file from this base.
    icon: appIconPath,
    extraResource: [coreBinaryPath],
    // The Forge Vite plugin normally packages only `.vite` and package.json.
    // node-pty is deliberately external in the main-process bundle, so keep
    // its runtime package (and its declared dependency) in the app archive.
    ignore: (file) => {
      if (!file) return false;
      return !(
        file.startsWith("/.vite") ||
        file === "/assets" ||
        file === "/assets/icon.png" ||
        file === "/assets/icon.ico" ||
        file === "/node_modules" ||
        file === "/node_modules/node-pty" ||
        file.startsWith("/node_modules/node-pty/") ||
        file === "/node_modules/node-addon-api" ||
        file.startsWith("/node_modules/node-addon-api/")
      );
    },
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
        const appBundlePath = platform === "darwin" && !outputPath.endsWith(".app")
          ? path.join(
              outputPath,
              readdirSync(outputPath).find((entry) => entry.endsWith(".app")) ?? "",
            )
          : outputPath;
        const resourcesPath = platform === "darwin"
          ? path.join(appBundlePath, "Contents", "Resources")
          : path.join(outputPath, "resources");
        const bundledCore = path.join(resourcesPath, coreBinaryName);
        if (!existsSync(bundledCore)) {
          throw new Error(`Packaged Luma core is missing from ${resourcesPath}.`);
        }
        const packagedPty = path.join(resourcesPath, "app.asar.unpacked", "node_modules", "node-pty");
        if (!existsSync(path.join(resourcesPath, "app.asar"))) {
          throw new Error(`Packaged application archive is missing from ${resourcesPath}.`);
        }
        if (!existsSync(packagedPty)) {
          throw new Error(`Packaged node-pty native module is missing from ${packagedPty}.`);
        }
      }
      console.log(`Verified bundled Luma core and node-pty in ${outputPaths.length} packaged app(s).`);
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
    new AutoUnpackNativesPlugin(),
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
