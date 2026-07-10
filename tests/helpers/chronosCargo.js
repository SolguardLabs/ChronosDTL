const childProcess = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..", "..");

function projectPath(...parts) {
  return path.join(root, ...parts);
}

function readProjectFile(...parts) {
  return fs.readFileSync(projectPath(...parts), "utf8");
}

function run(command, args, options = {}) {
  return childProcess.execFileSync(command, args, {
    cwd: root,
    encoding: "utf8",
    stdio: options.stdio ?? "pipe",
  });
}

function srcLineCount() {
  const src = projectPath("src");
  const stack = [src];
  let total = 0;
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(full);
      } else if (entry.isFile() && entry.name.endsWith(".rs")) {
        total += fs.readFileSync(full, "utf8").split(/\r?\n/).length;
      }
    }
  }
  return total;
}

module.exports = {
  projectPath,
  readProjectFile,
  run,
  srcLineCount,
};
