#!/usr/bin/env node
// Genera las llaves RSA desechables que usan las pruebas de OIDC (apps/server/tests/auth_oidc.rs) y su JWKS.
//
// Por qué se generan y no se suben: el repo nunca lleva llaves privadas (`*.pem` está en .gitignore y CI escanea
// secretos). Antes se guardaban solo en la máquina del autor, y en cualquier otra (incluido CI) `include_str!`
// no encontraba los archivos y las pruebas de Rust no compilaban. Ahora `pnpm verify` y CI las crean si faltan.
//
// Uso: node scripts/gen-test-keys.mjs [--force]   (sin --force no toca llaves existentes)
import { generateKeyPairSync } from "node:crypto";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const dir = join(dirname(fileURLToPath(import.meta.url)), "..", "apps/server/tests/fixtures");
const files = {
  test: join(dir, "oidc_test_key.pem"),
  other: join(dir, "oidc_other_key.pem"),
  jwks: join(dir, "oidc_jwks.json"),
};
const force = process.argv.includes("--force");
if (!force && Object.values(files).every(existsSync)) process.exit(0);

mkdirSync(dir, { recursive: true });
const pair = () => generateKeyPairSync("rsa", { modulusLength: 2048 });
const test = pair();
const other = pair(); // Llave que NO está en el JWKS: la prueba de firma inválida debe rechazarla.
const pem = (k) => k.privateKey.export({ type: "pkcs8", format: "pem" });
writeFileSync(files.test, pem(test));
writeFileSync(files.other, pem(other));
const { n, e } = test.publicKey.export({ format: "jwk" });
// kid "test-key" coincide con el header que firma auth_oidc.rs.
writeFileSync(files.jwks, JSON.stringify({ keys: [{ kty: "RSA", use: "sig", alg: "RS256", kid: "test-key", n, e }] }));
console.log("✓ Llaves de prueba OIDC generadas en apps/server/tests/fixtures/");
