import { exec } from "child_process";
import { platform } from "os";
import { resolve } from "path";

const bundlePath = resolve("src-tauri/target/release/bundle");
const commands = {
  win32: `start "" "${bundlePath}"`,
  darwin: `open "${bundlePath}"`,
  linux: `xdg-open "${bundlePath}"`,
};

const cmd = commands[platform()];
if (cmd) {
  exec(cmd, (err) => {
    if (err) console.error("Failed to open bundle folder:", err.message);
  });
} else {
  console.log("Bundle location:", bundlePath);
}

