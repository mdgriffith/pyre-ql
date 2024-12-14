import * as vscode from "vscode";
import { exec } from "child_process";
const path = require('path');


const pyreCommand = "/Users/mattgriffith/projects/vendr/pyre-ql/target/debug/pyre" // "pyre"


// Run the `pyre check` command and update diagnostics
export function checkErrors(document: vscode.TextDocument, diagnostics: vscode.DiagnosticCollection): void {
    // const filePath = document.uri.fsPath;
    
    const filePath = document.uri.fsPath;
    const cwd = path.dirname(path.dirname(filePath)); // Get the directory containing the file
    const command = `${pyreCommand} check --json`;
  
    exec(command, { cwd }, (error, stdout, stderr) => {
      if (error) {
        try {
          const errors = JSON.parse(stdout);
          
          const diagnosticList: vscode.Diagnostic[] = [];
          for (const err of errors) {
            const diagnostic = createDiagnosticFromPyreError(err);
            if (diagnostic) {
              diagnosticList.push(diagnostic);
            }
          }
          diagnostics.set(document.uri, diagnosticList);
        } catch (e) {
          console.error(e)
        }
      }
  
     
    });
  }

  // Convert a Pyre position (line, column) to a VSCode Position
  function fromPyrePosition(pos: { line: number; column: number }): vscode.Position {
    return new vscode.Position(pos.line - 1, pos.column - 1);
  }
  
  // Convert a single Pyre error to a VSCode Diagnostic
  function createDiagnosticFromPyreError(error: any): vscode.Diagnostic | null {
    if (error.locations.length > 0 && error.locations[0].primary.length > 0) {
      const location = error.locations[0];
      const start = fromPyrePosition(location.primary[0].start);
      const end = fromPyrePosition(location.primary[0].end);
      const range = new vscode.Range(start, end);
    
      return new vscode.Diagnostic(
        range,
        error.description,
        vscode.DiagnosticSeverity.Error // Use Error severity for red underlines
      );
    }
    return null
   
  }