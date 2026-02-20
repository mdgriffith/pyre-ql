import { spawn } from "child_process";

const pyreCommand = "/Users/griff/projects/pyre/target/debug/pyre"

export async function format(input: string, filepath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const pyreProcess = spawn(pyreCommand, ["format", filepath]);
    let stdout = "";
    let stderr = "";

    pyreProcess.stdout.on("data", (data) => {
      stdout += data.toString();
    });

    pyreProcess.stderr.on("data", (data) => {
      stderr += data.toString();
    });

    pyreProcess.on("close", (code) => {
      if (code === 0) {
        resolve(stdout);
      } else {
        reject(new Error(`Pyre format failed: ${stderr}`));
      }
    });

    pyreProcess.stdin.write(input);
    pyreProcess.stdin.end();
  });
}
