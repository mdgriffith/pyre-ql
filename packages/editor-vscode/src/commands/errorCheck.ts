import * as vscode from "vscode";
import { execFile } from "child_process";
import * as fs from "fs";
import * as path from "path";

const pyreCommand = "/Users/griff/projects/pyre/target/debug/pyre";

// Run the `pyre check` command and update diagnostics
export function checkErrors(document: vscode.TextDocument, diagnostics: vscode.DiagnosticCollection): void {
  const filePath = document.uri.fsPath;
  const cwd = findPyreProjectRoot(filePath);

  execFile(pyreCommand, ["check", "--json"], { cwd, maxBuffer: 10 * 1024 * 1024 }, (_error, stdout) => {
    if (!stdout || stdout.trim().length === 0) {
      diagnostics.clear();
      return;
    }

    try {
      const errors = JSON.parse(stdout);
      diagnostics.clear();

      const byFile = new Map<string, vscode.Diagnostic[]>();
      for (const err of errors) {
        const diagnostic = createDiagnosticFromPyreError(err);
        if (!diagnostic) {
          continue;
        }

        const resolvedPath = resolveErrorFilepath(cwd, err.filepath);
        if (!resolvedPath) {
          continue;
        }

        const uri = vscode.Uri.file(resolvedPath);
        const existing = byFile.get(uri.toString()) || [];
        existing.push(diagnostic);
        byFile.set(uri.toString(), existing);
      }

      for (const [uriString, fileDiagnostics] of byFile.entries()) {
        diagnostics.set(vscode.Uri.parse(uriString), fileDiagnostics);
      }
    } catch (e) {
      console.error(e);
    }
  });
}

function resolveErrorFilepath(cwd: string, filepath: string | undefined): string | null {
  if (!filepath || filepath.length === 0) {
    return null;
  }

  if (path.isAbsolute(filepath)) {
    return filepath;
  }

  return path.resolve(cwd, filepath);
}

// Convert a Pyre position (line, column) to a VSCode Position
function fromPyrePosition(pos: { line: number; column: number }): vscode.Position {
  return new vscode.Position(pos.line - 1, pos.column - 1);
}

function findPyreProjectRoot(filePath: string): string {
  let current = path.dirname(filePath);

  while (true) {
    const pyreDir = path.join(current, "pyre");
    if (fs.existsSync(pyreDir) && fs.statSync(pyreDir).isDirectory()) {
      return current;
    }

    const parent = path.dirname(current);
    if (parent === current) {
      return path.dirname(filePath);
    }

    current = parent;
  }
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
  return null;
}
