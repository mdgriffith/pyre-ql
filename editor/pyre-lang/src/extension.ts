import * as vscode from "vscode";
import { format } from "./commands/format";
import {checkErrors} from "./commands/errorCheck";

const pyre = vscode.window.createOutputChannel("Pyre");

export function activate(context: vscode.ExtensionContext) {


  // Register the formatting provider
  const formattingProvider = vscode.languages.registerDocumentFormattingEditProvider(
    "pyre",
    {
      async provideDocumentFormattingEdits(
        document: vscode.TextDocument,
      ): Promise<vscode.TextEdit[]> {
        try {
          pyre.appendLine("Formatting")
          const text = document.getText();
          const formattedText = await format(text, document.fileName);
         
          const fullRange = new vscode.Range(
            document.positionAt(0),
            document.positionAt(document.getText().length),
          );
          pyre.appendLine("Replacing")


          return [vscode.TextEdit.replace(fullRange, formattedText)];
        } catch (error) {
          pyre.appendLine(`Error formatting Pyre file: ${error}`)
          vscode.window.showErrorMessage(
            `Error formatting Pyre file: ${error}`,
          );
          return [];
        }
      },
    },
  );
  context.subscriptions.push(formattingProvider);



  // Dignostics
  const diagnostics = vscode.languages.createDiagnosticCollection('pyre');
  context.subscriptions.push(diagnostics);

  // Listen for document save events and trigger diagnostics
  vscode.workspace.onDidSaveTextDocument((document) => {
    if (document.languageId === 'pyre') {
      checkErrors(document, diagnostics);
    }
  });


}


// const pyreCommand = "/Users/mattgriffith/projects/vendr/pyre-ql/target/debug/pyre" // "pyre"

// async function formatPyre(input: string, filepath: string): Promise<string> {
//   return new Promise((resolve, reject) => {
//     const pyreProcess = spawn(pyreCommand, ["format", filepath]);
//     let stdout = "";
//     let stderr = "";

//     pyreProcess.stdout.on("data", (data) => {
//       stdout += data.toString();
//     });

//     pyreProcess.stderr.on("data", (data) => {
//       stderr += data.toString();
//       pyre.appendLine(`Err: ${data.toString()}`)
//     });

//     pyreProcess.on("close", (code) => {
//       pyre.appendLine(`Close: ${code}`)
//       pyre.appendLine(stdout)
//       if (code === 0) {
//         resolve(stdout);
//       } else {
//         reject(new Error(`Pyre format failed: ${stderr}`));
//       }
//     });

//     pyreProcess.stdin.write(input);
//     pyreProcess.stdin.end();
//   });
// }

export function deactivate() { }
