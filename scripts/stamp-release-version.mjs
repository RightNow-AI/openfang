import fs from 'node:fs';

const version = process.env.VERSION;
if (!version) throw new Error('VERSION is required');

function replaceWorkspacePackageVersion(content) {
  const lines = content.split('\n');
  let inWorkspacePackage = false;

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];

    if (/^\s*\[workspace\.package\]\s*$/.test(line)) {
      inWorkspacePackage = true;
      continue;
    }

    if (inWorkspacePackage && /^\s*\[.*\]\s*$/.test(line)) {
      break;
    }

    if (inWorkspacePackage && /^\s*version\s*=\s*"[^"]+"\s*$/.test(line)) {
      lines[i] = `version = "${version}"`;
      return lines.join('\n');
    }
  }

  throw new Error('failed to stamp workspace.package version in Cargo.toml');
}

const cargoPath = 'Cargo.toml';
const cargo = fs.readFileSync(cargoPath, 'utf8');
fs.writeFileSync(cargoPath, replaceWorkspacePackageVersion(cargo));

const tauriPath = 'crates/openfang-desktop/tauri.conf.json';
if (fs.existsSync(tauriPath)) {
  const tauri = JSON.parse(fs.readFileSync(tauriPath, 'utf8'));
  tauri.version = version;
  fs.writeFileSync(tauriPath, `${JSON.stringify(tauri, null, 2)}\n`);
}
