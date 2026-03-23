'use strict';

const fs = require('node:fs');
const path = require('node:path');

const INDEX_PATH = path.join(process.cwd(), 'data', 'founders-kit-index.sample.json');

function loadFoundersKitIndex() {
  try {
    const raw = fs.readFileSync(INDEX_PATH, 'utf8');
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed?.entries) ? parsed.entries : [];
  } catch {
    return [];
  }
}

function normalize(text) {
  return String(text ?? '').trim().toLowerCase();
}

function tokenize(text) {
  return normalize(text)
    .split(/[^a-z0-9]+/)
    .filter((token) => token.length >= 2);
}

function scoreEntry(entry, tokens, categoryFilter) {
  let score = 0;
  const title = normalize(entry.title);
  const description = normalize(entry.description);
  const category = normalize(entry.category);
  const subcategory = normalize(entry.subcategory);
  const tags = Array.isArray(entry.tags) ? entry.tags.map(normalize) : [];

  if (categoryFilter && category === normalize(categoryFilter)) {
    score += 50;
  }

  for (const token of tokens) {
    if (title.includes(token)) score += 10;
    if (description.includes(token)) score += 5;
    if (subcategory.includes(token)) score += 4;
    if (category.includes(token)) score += 3;
    if (tags.some((tag) => tag.includes(token))) score += 6;
  }

  return score;
}

function searchFoundersKit({ query = '', category = null, limit = 8 }) {
  const entries = loadFoundersKitIndex();
  const tokens = tokenize(query);
  const normalizedCategory = normalize(category);

  const filtered = normalizedCategory
    ? entries.filter((entry) => normalize(entry.category) === normalizedCategory)
    : entries;

  return filtered
    .map((entry) => ({ entry, score: scoreEntry(entry, tokens, normalizedCategory) }))
    .filter(({ score }) => score > 0 || tokens.length === 0)
    .sort((a, b) => b.score - a.score || String(a.entry.title).localeCompare(String(b.entry.title)))
    .slice(0, limit)
    .map(({ entry }) => entry);
}

module.exports = {
  loadFoundersKitIndex,
  searchFoundersKit,
};