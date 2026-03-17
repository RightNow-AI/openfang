import { describe, it, expect } from 'vitest';
import { validateSpawnName } from '../spawn-validation';

describe('validateSpawnName', () => {
  it('rejects empty names', () => {
    expect(validateSpawnName('').error).toBeTruthy();
    expect(validateSpawnName('   ').error).toBeTruthy();
  });

  it('rejects names longer than 64 characters', () => {
    const long = 'a'.repeat(65);
    expect(validateSpawnName(long).error).toBeTruthy();
    // 64 chars should pass
    expect(validateSpawnName('a'.repeat(64)).name).toBe('a'.repeat(64));
  });

  it('rejects control characters', () => {
    expect(validateSpawnName('hello\x00world').error).toBeTruthy();
    expect(validateSpawnName('line\nbreak').error).toBeTruthy();
    expect(validateSpawnName('tab\there').error).toBeTruthy();
  });

  it('rejects filesystem-unsafe characters', () => {
    for (const ch of ['<', '>', ':', '"', '/', '\\', '|', '?', '*']) {
      expect(validateSpawnName(`bad${ch}name`).error).toBeTruthy();
    }
  });

  it('rejects leading dots', () => {
    expect(validateSpawnName('.hidden').error).toBeTruthy();
  });

  it('rejects trailing dots', () => {
    expect(validateSpawnName('trailing.').error).toBeTruthy();
  });

  it('trims surrounding whitespace and returns the trimmed name', () => {
    const result = validateSpawnName('  my agent  ');
    expect(result.name).toBe('my agent');
  });

  it('accepts emoji and normal unicode characters', () => {
    expect(validateSpawnName('🤖 researcher').name).toBe('🤖 researcher');
    expect(validateSpawnName('エージェント').name).toBe('エージェント');
  });

  it('accepts a normal valid name', () => {
    const result = validateSpawnName('my-custom-agent');
    expect(result.name).toBe('my-custom-agent');
    expect(result.error).toBeUndefined();
  });
});
