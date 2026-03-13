const { createHash, randomUUID } = require("node:crypto");

const { upsertUser } = require("./session-store");

const SESSION_COOKIE_NAME = "openfang_example_session";
const SESSION_COOKIE_MAX_AGE = 60 * 60 * 24 * 30;

function hashValue(value) {
  return createHash("sha256").update(value).digest("hex");
}

function makeUserId(sessionToken) {
  return `user_${hashValue(sessionToken).slice(0, 24)}`;
}

async function resolveUserIdentity(request) {
  const cookieValue = request.cookies.get(SESSION_COOKIE_NAME)?.value?.trim();
  const sessionToken = cookieValue || randomUUID();
  const userId = makeUserId(sessionToken);

  await upsertUser({
    user_id: userId,
    auth_provider: "custom-session-cookie",
    session_token_hash: hashValue(sessionToken),
  });

  return {
    userId,
    authProvider: "custom-session-cookie",
    sessionToken,
    isNew: !cookieValue,
  };
}

function applyIdentityCookie(response, identity) {
  if (!identity?.isNew) {
    return response;
  }

  response.cookies.set(SESSION_COOKIE_NAME, identity.sessionToken, {
    httpOnly: true,
    sameSite: "lax",
    secure: false,
    path: "/",
    maxAge: SESSION_COOKIE_MAX_AGE,
  });

  return response;
}

module.exports = {
  applyIdentityCookie,
  resolveUserIdentity,
};