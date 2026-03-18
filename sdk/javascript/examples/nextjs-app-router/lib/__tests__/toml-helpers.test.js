import { describe, it, expect } from 'vitest';
import {
  extractTomlField,
  extractTomlMultiline,
  patchTomlName,
} from '../toml-helpers.js';

const SIMPLE_TOML = `
name = "my-agent"
description = "does stuff"
`;

const MULTILINE_TOML = `
name = "my-agent"
system_prompt = """
You are a helpful assistant.
Be concise.
"""
`;

// A multiline value containing an escaped quote (\" is TOML for a literal ")
const ESCAPED_QUOTE_TOML = `
name = "my-agent"
bio = """
She said \\"hello\\" and left.
"""
`;

// TOML 1.0 §2.4.5: four consecutive quotes = one content quote + closing """
const FOUR_QUOTE_TOML = `
name = "my-agent"
quote = """"ends with one quote""""
`;

describe('extractTomlField', () => {
  it('extracts a simple quoted field', () => {
    expect(extractTomlField(SIMPLE_TOML, 'name')).toBe('my-agent');
    expect(extractTomlField(SIMPLE_TOML, 'description')).toBe('does stuff');
  });

  it('returns empty string when field is absent', () => {
    expect(extractTomlField(SIMPLE_TOML, 'missing_field')).toBe('');
  });

  it('returns empty string for null/undefined input', () => {
    expect(extractTomlField(null, 'name')).toBe('');
    expect(extractTomlField(undefined, 'name')).toBe('');
    expect(extractTomlField('', 'name')).toBe('');
  });

  it('does not match multi-line triple-quoted fields as single-line', () => {
    // system_prompt uses """, so single-line extraction returns ''
    expect(extractTomlField(MULTILINE_TOML, 'system_prompt')).toBe('');
  });
});

describe('extractTomlMultiline', () => {
  it('extracts a triple-quoted multi-line field', () => {
    const result = extractTomlMultiline(MULTILINE_TOML, 'system_prompt');
    expect(result).toBe('You are a helpful assistant.\nBe concise.');
  });

  it('falls back to single-line extraction when field is not triple-quoted', () => {
    expect(extractTomlMultiline(SIMPLE_TOML, 'name')).toBe('my-agent');
  });

  it('returns empty string when field is absent', () => {
    expect(extractTomlMultiline(SIMPLE_TOML, 'missing_field')).toBe('');
  });

  it('returns empty string for null/undefined input', () => {
    expect(extractTomlMultiline(null, 'name')).toBe('');
  });

  it('correctly processes escaped quotes inside a triple-quoted string', () => {
    // ESCAPED_QUOTE_TOML has bio = """\nShe said \"hello\" and left.\n"""
    // \" is a TOML escape for a literal double-quote
    const result = extractTomlMultiline(ESCAPED_QUOTE_TOML, 'bio');
    expect(result).toBe('She said "hello" and left.');
  });

  it('handles TOML 1.0 four-consecutive-quote sequence (one content quote before closing)', () => {
    // FOUR_QUOTE_TOML: quote = """"ends with one quote""""
    // " opening """ then " is content, then "ends with one quote", then
    // """" closing:  " is content + """ closes.
    // Correct parse: "ends with one quote" (both surrounding quotes are content)
    const result = extractTomlMultiline(FOUR_QUOTE_TOML, 'quote');
    expect(result).toBe('"ends with one quote"');
  });
});

describe('patchTomlName', () => {
  it('replaces the name field in place', () => {
    const result = patchTomlName(SIMPLE_TOML, 'new-name');
    expect(result).toContain('name = "new-name"');
    expect(result).not.toContain('name = "my-agent"');
  });

  it('preserves all other fields', () => {
    const result = patchTomlName(SIMPLE_TOML, 'x');
    expect(result).toContain('description = "does stuff"');
  });

  it('escapes double-quotes in the new name', () => {
    const result = patchTomlName(SIMPLE_TOML, 'say "hello"');
    expect(result).toContain('name = "say \\"hello\\""');
  });

  it('escapes backslashes in the new name', () => {
    const result = patchTomlName(SIMPLE_TOML, 'path\\value');
    expect(result).toContain('name = "path\\\\value"');
  });

  it('handles a name with both backslash and quote', () => {
    const result = patchTomlName(SIMPLE_TOML, 'a\\"b');
    expect(result).toContain('name = "a\\\\\\"b"');
  });
});
