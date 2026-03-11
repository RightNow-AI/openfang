---
name: browser-automation
version: "1.0.0"
description: Playwright-based browser automation patterns for autonomous web interaction
author: Claw
tags: [browser, automation, playwright, web, scraping]
tools:
  [
    browser_navigate,
    browser_click,
    browser_type,
    browser_screenshot,
    browser_read_page,
    browser_close,
  ]
runtime: prompt_only
---

# Browser Automation Skill

## Playwright CSS Selector Reference

### Basic Selectors

| Selector     | Description  | Example                  |
| ------------ | ------------ | ------------------------ |
| `#id`        | By ID        | `#checkout-btn`          |
| `.class`     | By class     | `.add-to-cart`           |
| `tag`        | By element   | `button`, `input`        |
| `[attr=val]` | By attribute | `[data-testid="submit"]` |
| `tag.class`  | Combined     | `button.primary`         |

### Form Selectors

| Selector                 | Use Case              |
| ------------------------ | --------------------- |
| `input[type="email"]`    | Email fields          |
| `input[type="password"]` | Password fields       |
| `input[type="search"]`   | Search boxes          |
| `input[name="q"]`        | Google/search query   |
| `textarea`               | Multi-line text areas |
| `select[name="country"]` | Dropdown menus        |
| `input[type="checkbox"]` | Checkboxes            |
| `input[type="radio"]`    | Radio buttons         |
| `button[type="submit"]`  | Submit buttons        |

### Navigation Selectors

| Selector                | Use Case              |
| ----------------------- | --------------------- |
| `a[href*="cart"]`       | Cart links            |
| `a[href*="checkout"]`   | Checkout links        |
| `a[href*="login"]`      | Login links           |
| `nav a`                 | Navigation menu links |
| `.breadcrumb a`         | Breadcrumb links      |
| `[role="navigation"] a` | ARIA nav links        |

### E-commerce Selectors

| Selector                              | Use Case            |
| ------------------------------------- | ------------------- |
| `.product-price`, `[data-price]`      | Product prices      |
| `.add-to-cart`, `#add-to-cart`        | Add to cart buttons |
| `.cart-total`, `.order-total`         | Cart total          |
| `.quantity`, `input[name="quantity"]` | Quantity selectors  |
| `.checkout-btn`, `#checkout`          | Checkout buttons    |

## Common Workflows

### Product Search & Purchase

```
1. browser_navigate → store homepage
2. browser_type → search box with product name
3. browser_click → search button or press Enter
4. browser_read_page → scan results
5. browser_click → desired product
6. browser_read_page → verify product details & price
7. browser_click → "Add to Cart"
8. browser_navigate → cart page
9. browser_read_page → verify cart contents & total
10. STOP → Report to user, wait for approval
11. browser_click → "Proceed to Checkout" (only after approval)
```

### Account Login

```
1. browser_navigate → login page
2. browser_type → email/username field
3. browser_type → password field
4. browser_click → login/submit button
5. browser_read_page → verify successful login
```

### Form Submission

```
1. browser_navigate → form page
2. browser_read_page → understand form structure
3. browser_type → fill each field sequentially
4. browser_click → checkboxes/radio buttons as needed
5. browser_screenshot → visual verification before submit
6. browser_click → submit button
7. browser_read_page → verify confirmation
```

### Price Comparison

```
1. For each store:
   a. browser_navigate → store URL
   b. browser_type → search query
   c. browser_read_page → extract prices
   d. memory_store → save price data
2. memory_recall → compare all prices
3. Report findings to user
```

## Error Recovery Strategies

| Error             | Recovery                                                |
| ----------------- | ------------------------------------------------------- |
| Element not found | Try alternative selector, use visible text, scroll page |
| Page timeout      | Retry navigation, check URL                             |
| Login required    | Inform user, ask for credentials                        |
| CAPTCHA           | Cannot solve — inform user                              |
| Pop-up/modal      | Click dismiss/close button first                        |
| Cookie consent    | Click "Accept" or dismiss banner                        |
| Rate limited      | Wait 30s, retry                                         |
| Wrong page        | Use browser_read_page to verify, navigate back          |

## Security Checklist

- Verify domain before entering credentials
- Never store passwords in memory_store
- Check for HTTPS before submitting sensitive data
- Report suspicious redirects to user
- Never auto-approve financial transactions
- Warn about phishing indicators (misspelled domains, unusual URLs)
