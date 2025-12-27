/**
 * Browser-based challenge solving for Cloudflare and similar protections
 */

import puppeteer, { type Browser, type Page } from "puppeteer";

export interface ChallengeResult {
  cookies: string;
  userAgent: string;
}

// Shared browser instance that persists after challenge solving
let sharedBrowser: Browser | null = null;
// Shared page that maintains cookies from challenge solving
let sharedPage: Page | null = null;

/**
 * Check if we have an active browser session
 */
export function hasBrowserSession(): boolean {
  return sharedPage !== null && !sharedPage.isClosed();
}

/**
 * Close the shared browser instance
 */
export async function closeBrowser(): Promise<void> {
  if (sharedPage) {
    try {
      await sharedPage.close();
    } catch {
      // Ignore
    }
    sharedPage = null;
  }
  if (sharedBrowser) {
    await sharedBrowser.close();
    sharedBrowser = null;
  }
}

/**
 * Fetch a page using Puppeteer (bypasses TLS fingerprinting)
 * Uses the same authenticated page - must be called sequentially (concurrency 1)
 */
export async function fetchWithPuppeteer(
  url: string,
): Promise<{ html: string; status: number }> {
  if (!sharedPage || sharedPage.isClosed()) {
    throw new Error("No browser session available. Call solveChallenge first.");
  }

  try {
    const response = await sharedPage.goto(url, {
      waitUntil: "networkidle2",
      timeout: 30000,
    });

    const status = response?.status() ?? 0;
    const html = await sharedPage.content();

    return { html, status };
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    throw new Error(`Failed to fetch ${url}: ${msg}`);
  }
}

/**
 * Opens a browser window for the user to solve any challenges (Cloudflare, captcha, etc.)
 * Waits until the page loads successfully, then keeps the same browser for scraping.
 * The browser stays open (visible) to maintain the authenticated session.
 */
export async function solveChallenge(url: string): Promise<ChallengeResult> {
  console.log("Opening browser for challenge solving...");
  console.log("Please solve any captcha or wait for the page to load.");
  console.log("The browser window will stay open during scraping.\n");

  // Close any existing browser
  await closeBrowser();

  // Launch visible browser for user interaction - this same browser will be reused
  sharedBrowser = await puppeteer.launch({
    headless: false,
    defaultViewport: { width: 1280, height: 800 },
    args: ["--start-maximized"],
  });

  sharedPage = await sharedBrowser.newPage();
  const userAgent = await sharedBrowser.userAgent();

  // Navigate to the URL
  await sharedPage.goto(url, { waitUntil: "domcontentloaded" });

  // Wait for the challenge to be solved
  await waitForChallengeResolved(sharedPage, url);

  // Extract cookies from the target domain
  const urlObj = new URL(url);
  const cookies = await sharedPage.cookies(urlObj.origin);
  const cookieString = cookies.map((c) => `${c.name}=${c.value}`).join("; ");

  console.log(`\nChallenge solved! Extracted ${cookies.length} cookies.`);

  // Keep the page open - it maintains the cookies for subsequent requests
  console.log(
    "Browser session ready. Starting scraping (use --concurrency 1)...\n",
  );

  return {
    cookies: cookieString,
    userAgent,
  };
}

/**
 * Wait until the challenge is resolved by checking for cf_clearance cookie
 */
async function waitForChallengeResolved(
  page: Page,
  _url: string,
): Promise<void> {
  const maxWaitTime = 120000; // 2 minutes max
  const checkInterval = 1000;
  const startTime = Date.now();

  while (Date.now() - startTime < maxWaitTime) {
    // Check for cf_clearance cookie - this is the key indicator
    const cookies = await page.cookies();
    const hasClearance = cookies.some((c) => c.name === "cf_clearance");

    // Also check page content for challenge indicators
    const pageState = await page.evaluate(() => {
      const title = document.title.toLowerCase();
      const html = document.documentElement.innerHTML.toLowerCase();

      const challengeIndicators = [
        "just a moment",
        "checking your browser",
        "please wait",
        "ddos protection",
        "challenge-running",
        "cf-challenge",
        "turnstile",
      ];

      const hasChallenge = challengeIndicators.some(
        (indicator) => title.includes(indicator) || html.includes(indicator),
      );

      return { hasChallenge, title };
    });

    if (hasClearance) {
      // Wait a bit more for final cookies
      await new Promise((resolve) => setTimeout(resolve, 2000));
      console.log("cf_clearance cookie detected.");
      return;
    }

    if (!pageState.hasChallenge && cookies.length > 0) {
      // Page looks good, wait a bit more for cf_clearance
      await new Promise((resolve) => setTimeout(resolve, 3000));
      const finalCookies = await page.cookies();
      if (finalCookies.some((c) => c.name === "cf_clearance")) {
        console.log("cf_clearance cookie detected after wait.");
        return;
      }
      // Proceed even without cf_clearance if page seems resolved
      console.log("Page appears resolved, proceeding...");
      return;
    }

    await new Promise((resolve) => setTimeout(resolve, checkInterval));
  }

  console.warn(
    "Timeout waiting for challenge resolution. Proceeding anyway...",
  );
}
