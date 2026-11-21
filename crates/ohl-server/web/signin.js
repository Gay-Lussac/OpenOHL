// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

// The web viewer's "Sign in with your organisation" (2026-11-21): the
// authorization code flow with PKCE (RFC 7636, S256) against the network's
// identity provider, the page's own address as the redirect. The same flow
// as OHL-Design's sign-in window, in the browser:
//
// - `begin` reads the provider's discovery document (its `issuer` must be
//   the one the server named), makes a verifier, a `state` and a `nonce`
//   and returns the provider's sign-in address and what the page must keep
//   across the provider's page (`pending`: one-time values, no token);
// - `finish` checks the `state`, exchanges the code at the token endpoint
//   (the provider must allow the page's origin there: CORS) and returns the
//   ID token after checking its issuer, audience and nonce — the server
//   checks its signature and times;
// - `renew` gets a new ID token with the refresh token.
//
// Tokens are kept in the page's memory only (the caller's variables): never
// written to storage, gone when the tab closes or reloads. No password
// passes through the page. Runs in a browser or in Node.js (the tests).
"use strict";

(() => {
  // How long the provider's page may take, ms.
  const PENDING_MAX = 10 * 60 * 1000;
  // An ID token is renewed when older than this, ms (a server takes
  // tokens issued within the hour).
  const RENEW_AFTER = 50 * 60 * 1000;
  const CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

  function random(n) {
    // 66 characters: bytes of 198 and more left out (no bias).
    const bytes = new Uint8Array(2 * n);
    let out = "";
    while (out.length < n) {
      globalThis.crypto.getRandomValues(bytes);
      for (const b of bytes) {
        if (b < 198 && out.length < n) out += CHARS[b % 66];
      }
    }
    return out;
  }

  function b64url(bytes) {
    let text = "";
    for (const b of new Uint8Array(bytes)) text += String.fromCharCode(b);
    return btoa(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  }

  function unb64url(text) {
    const t = text.replace(/-/g, "+").replace(/_/g, "/");
    const bin = atob(t + "===".slice((t.length + 3) % 4));
    return new TextDecoder().decode(Uint8Array.from(bin, (c) => c.charCodeAt(0)));
  }

  async function challenge(verifier) {
    const digest = await globalThis.crypto.subtle.digest(
      "SHA-256",
      new TextEncoder().encode(verifier),
    );
    return b64url(digest);
  }

  // Whether `url` is plain HTTP to this computer (a provider for tests).
  function loopback(url) {
    try {
      const u = new URL(url);
      const host = u.hostname.replace(/^\[|\]$/g, "");
      return (
        u.protocol === "http:" &&
        (host === "localhost" || host === "::1" || /^127\.\d+\.\d+\.\d+$/.test(host))
      );
    } catch (e) {
      return false;
    }
  }

  function checkUrl(what, url) {
    if (typeof url !== "string" || !(url.startsWith("https://") || loopback(url))) {
      throw new Error(`${what} ${url}: HTTPS only`);
    }
  }

  function originOf(url) {
    return new URL(url).origin;
  }

  // The provider's endpoints, from its discovery document.
  async function discover(issuer) {
    checkUrl("the provider", issuer);
    const url = `${issuer.replace(/\/+$/, "")}/.well-known/openid-configuration`;
    let r;
    try {
      r = await fetch(url, { credentials: "omit", redirect: "error" });
    } catch (e) {
      throw new Error(
        `the provider's discovery document could not be read (${url}): the provider must ` +
          "allow this page's address (CORS), and the network's server must name it",
      );
    }
    if (!r.ok) throw new Error(`the provider's discovery document: HTTP ${r.status}`);
    const d = await r.json();
    if (d.issuer !== issuer) {
      throw new Error(`the provider names itself ${d.issuer}, not ${issuer}`);
    }
    checkUrl("the provider's sign-in page", d.authorization_endpoint);
    checkUrl("the provider's token endpoint", d.token_endpoint);
    if (originOf(d.token_endpoint) !== originOf(issuer)) {
      // The page's security policy names the issuer's origin only.
      throw new Error(
        `the provider's token endpoint (${originOf(d.token_endpoint)}) is not at its issuer's ` +
          `address (${originOf(issuer)}): the web viewer cannot reach it; sign in with ` +
          "OHL-Design, or join with your invitation",
      );
    }
    return { authorization: d.authorization_endpoint, token: d.token_endpoint };
  }

  // Starts a sign-in with `offer` ({issuer, client_id}, the server's
  // answer), coming back to `redirect` (the page's own address): the
  // provider's sign-in address and the one-time values to keep until the
  // page is back (`extra` kept with them: the user name).
  async function begin(offer, redirect, extra, now) {
    const endpoints = await discover(offer.issuer);
    const verifier = random(64);
    const state = random(32);
    const nonce = random(32);
    const q = new URLSearchParams({
      response_type: "code",
      client_id: offer.client_id,
      redirect_uri: redirect,
      scope: "openid",
      state,
      nonce,
      code_challenge: await challenge(verifier),
      code_challenge_method: "S256",
    });
    const sep = endpoints.authorization.includes("?") ? "&" : "?";
    return {
      url: `${endpoints.authorization}${sep}${q}`,
      pending: {
        issuer: offer.issuer,
        client_id: offer.client_id,
        token_endpoint: endpoints.token,
        redirect,
        verifier,
        state,
        nonce,
        started: now === undefined ? Date.now() : now,
        extra: extra || null,
      },
    };
  }

  // The provider's answer in the page's address (`search`, as
  // `location.search`): {code, state} or {error, error_description,
  // state}; null when it holds neither.
  function answer(search) {
    const q = new URLSearchParams(search || "");
    if (!q.has("code") && !q.has("error")) return null;
    return {
      code: q.get("code"),
      state: q.get("state"),
      error: q.get("error"),
      error_description: q.get("error_description"),
    };
  }

  function claims(idToken) {
    const parts = String(idToken).split(".");
    if (parts.length !== 3) throw new Error("the provider's ID token is not a JWT");
    return JSON.parse(unb64url(parts[1]));
  }

  async function token(endpoint, form) {
    let r;
    try {
      r = await fetch(endpoint, {
        method: "POST",
        headers: { "Content-Type": "application/x-www-form-urlencoded" },
        body: new URLSearchParams(form).toString(),
        credentials: "omit",
        redirect: "error",
      });
    } catch (e) {
      throw new Error(
        "the provider's token endpoint could not be reached from this page: the provider " +
          "must allow this page's address there (CORS)",
      );
    }
    let body = {};
    try {
      body = await r.json();
    } catch (e) {
      // Said below.
    }
    if (!r.ok || typeof body.id_token !== "string") {
      throw new Error(`the provider refused the sign-in: ${body.error || `HTTP ${r.status}`}`);
    }
    return body;
  }

  function signedIn(pending, body, now) {
    const c = claims(body.id_token);
    const aud = Array.isArray(c.aud) ? c.aud : [c.aud];
    if (c.iss !== pending.issuer) throw new Error(`the ID token is of ${c.iss}`);
    if (!aud.includes(pending.client_id)) throw new Error("the ID token is for another client");
    if (pending.nonce && c.nonce !== pending.nonce) {
      throw new Error("the ID token is not this sign-in's (its nonce)");
    }
    return {
      issuer: pending.issuer,
      client_id: pending.client_id,
      token_endpoint: pending.token_endpoint,
      subject: String(c.sub || ""),
      name: c.preferred_username || c.name || null,
      id_token: body.id_token,
      refresh_token: typeof body.refresh_token === "string" ? body.refresh_token : null,
      got: now === undefined ? Date.now() : now,
    };
  }

  // Finishes the sign-in begun with `pending` on the provider's answer
  // `ans` (see `answer`): the ID token, checked.
  async function finish(pending, ans, now) {
    const t = now === undefined ? Date.now() : now;
    if (!pending || !ans) throw new Error("no sign-in under way");
    if (ans.state !== pending.state) {
      throw new Error("this answer is not this sign-in's (its state): sign in again");
    }
    if (ans.error) {
      throw new Error(`the provider refused: ${ans.error_description || ans.error}`);
    }
    if (!(t - pending.started >= 0 && t - pending.started <= PENDING_MAX)) {
      throw new Error("the sign-in took more than 10 minutes: sign in again");
    }
    const body = await token(pending.token_endpoint, {
      grant_type: "authorization_code",
      code: ans.code || "",
      redirect_uri: pending.redirect,
      client_id: pending.client_id,
      code_verifier: pending.verifier,
    });
    return signedIn(pending, body, t);
  }

  // A new ID token with the refresh token (null: none was given).
  async function renew(s, now) {
    if (!s || !s.refresh_token) return null;
    const body = await token(s.token_endpoint, {
      grant_type: "refresh_token",
      refresh_token: s.refresh_token,
      client_id: s.client_id,
    });
    const next = signedIn({ issuer: s.issuer, client_id: s.client_id, nonce: null }, body, now);
    next.token_endpoint = s.token_endpoint;
    if (!next.refresh_token) next.refresh_token = s.refresh_token;
    return next;
  }

  function stale(s, now) {
    return (now === undefined ? Date.now() : now) - s.got > RENEW_AFTER;
  }

  globalThis.OhlSignIn = { begin, answer, finish, renew, stale, claims, PENDING_MAX };
})();
