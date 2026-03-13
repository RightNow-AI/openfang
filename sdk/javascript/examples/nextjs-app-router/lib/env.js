function readRequiredString(name, fallback) {
  const raw = process.env[name];
  const value = typeof raw === "string" && raw.trim() ? raw.trim() : fallback;

  if (!value) {
    throw new Error(`Missing required environment variable: ${name}`);
  }

  return value;
}

function readPositiveInteger(name, fallback) {
  const raw = process.env[name];
  if (raw == null || raw === "") {
    return fallback;
  }

  const parsed = Number.parseInt(String(raw), 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }

  return parsed;
}

const env = Object.freeze({
  OPENFANG_BASE_URL: readRequiredString(
    "OPENFANG_BASE_URL",
    "http://127.0.0.1:50051",
  ),
  OPENFANG_API_KEY: String(process.env.OPENFANG_API_KEY || "").trim(),
  OPENFANG_DEFAULT_TEMPLATE: readRequiredString(
    "OPENFANG_DEFAULT_TEMPLATE",
    "assistant",
  ),
  OPENFANG_TIMEOUT_MS: readPositiveInteger("OPENFANG_TIMEOUT_MS", 120000),
});

module.exports = { env };