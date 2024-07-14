import * as vscode from "vscode";
import { spawn } from "child_process";

export function activate(context: vscode.ExtensionContext) {
  // Register the formatting provider
  const provider = vscode.languages.registerDocumentFormattingEditProvider(
    "pyre",
    {
      async provideDocumentFormattingEdits(
        document: vscode.TextDocument,
      ): Promise<vscode.TextEdit[]> {
        try {
          const text = document.getText();
          const formattedText = await formatPyre(text, document.fileName);

          const fullRange = new vscode.Range(
            document.positionAt(0),
            document.positionAt(document.getText().length),
          );

          return [vscode.TextEdit.replace(fullRange, formattedText)];
        } catch (error) {
          vscode.window.showErrorMessage(
            `Error formatting Pyre file: ${error}`,
          );
          return [];
        }
      },
    },
  );

  // Register the command
  let disposable = vscode.commands.registerCommand("pyre.format", () => {
    const editor = vscode.window.activeTextEditor;
    if (editor) {
      vscode.commands.executeCommand("editor.action.formatDocument");
    }
  });

  context.subscriptions.push(provider, disposable);
}

async function formatPyre(input: string, filepath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const pyreProcess = spawn("pyre", ["format", filepath]);
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

export function deactivate() {}
