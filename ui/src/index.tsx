import { runCli } from "./cli";

void runCli(process.argv.slice(2)).catch((error) => {
  process.stderr.write(`luma: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
