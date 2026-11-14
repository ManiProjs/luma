import { runCli } from "./cli";
import { userError } from "./errors";

void runCli(process.argv.slice(2)).catch((error) => {
  process.stderr.write(`luma: ${userError(error)}\n`);
  process.exitCode = 1;
});
