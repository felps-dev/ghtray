# Phase 1: Data Exploration — Discoveries

## GraphQL Query Strategy

**Single combined query works.** All four search buckets can be fetched in one `gh api graphql` call.

### Cost Analysis

- Single mega-query cost: **5 points**
- GitHub GraphQL rate limit: **5,000 points/hour**
- At 2-minute polling: 30 queries/hour = **150 points/hour** (3% of budget)
- At 30-second polling (floor): 120 queries/hour = **600 points/hour** (12% of budget)
- Conclusion: extremely sustainable, even with aggressive polling

### Search Queries Used

| Alias            | Search Query                                                      | Purpose                                                    |
| ---------------- | ----------------------------------------------------------------- | ---------------------------------------------------------- |
| `needsReview`    | `is:pr is:open review-requested:@me`                              | PRs where my review is pending                             |
| `authored`       | `is:pr is:open author:@me`                                        | My open PRs (drafts + non-drafts)                          |
| `reviewedByMe`   | `is:pr is:open reviewed-by:@me -author:@me -review-requested:@me` | PRs I reviewed but no longer have a pending review request |
| `recentlyMerged` | `is:pr is:merged author:@me merged:>{date}`                       | My recently merged PRs                                     |

### Key Fields Per PR

```
id                    — Unique node ID (for caching)
number                — PR number
title                 — PR title
url                   — Browser URL
isDraft               — Draft status
createdAt / updatedAt — Timestamps
repository.nameWithOwner — e.g., "organizationName/repositoryName"
author.login          — PR author
author.avatarUrl      — For UI display
reviewDecision        — APPROVED | CHANGES_REQUESTED | REVIEW_REQUIRED | null
additions / deletions — Diff size
mergedAt              — Only on merged PRs
```

### Nested Connections

**`latestReviews(first: 10)`** — Returns the latest review from each reviewer (deduplicated). States:

- `APPROVED`
- `CHANGES_REQUESTED`
- `COMMENTED`
- `DISMISSED`
- `PENDING`

**`reviewRequests(first: 10)`** — Currently pending review requests. Can be `User` or `Team`.

**`commits(last: 1)`** — Last commit with:

- `oid` — SHA for detecting new commits
- `committedDate` — For comparing against review timestamps
- `statusCheckRollup.state` — CI status: `SUCCESS`, `FAILURE`, `PENDING`, `ERROR`, `EXPECTED`

**`timelineItems(itemTypes: [REVIEW_REQUESTED_EVENT], last: 5)`** — Review request events with timestamps. Useful for detecting re-requests (when `createdAt` > last review `submittedAt`).

## Categorization Logic (Refined from Real Data)

### For `needsReview` search results → "Needs Your Review"

A PR lands here if review is requested from me. But we can sub-categorize:

- **Never reviewed**: No review from me exists in `latestReviews`
- **New commits since review**: My latest review exists, but `commits.last.committedDate` > my review `submittedAt`
- **Re-requested**: A `ReviewRequestedEvent` for me exists with `createdAt` > my latest review `submittedAt`

All three sub-cases = "Needs Your Review" bucket.

### For `authored` search results → Multiple buckets

1. **Drafts**: `isDraft == true`
2. **Approved**: `reviewDecision == "APPROVED"`
3. **Returned to You**: `reviewDecision == "CHANGES_REQUESTED"`
4. **Waiting for Reviewers**: Everything else (no reviews, or only comments, pending review requests)

### For `reviewedByMe` search results → "Waiting for Author"

PRs I reviewed that are NOT in my review-requested set. Check:

- My latest review was `CHANGES_REQUESTED` AND no new commits since → "Waiting for Author"
- My latest review was `APPROVED` → just informational, could skip or show in a lighter section
- My latest review was `COMMENTED` → ambiguous, treat as "Waiting for Author" if `reviewDecision == CHANGES_REQUESTED`

### For `recentlyMerged` → "Recently Merged"

Straightforward — just display with `mergedAt` timestamp.

## Real-World Volume (felps-dev account, 2026-02-24)

| Bucket                    | Count                       |
| ------------------------- | --------------------------- |
| Needs review (requested)  | 12                          |
| My open PRs               | 10 (5 non-draft, 5 unknown) |
| Reviewed by me (watching) | 13                          |
| Recently merged (7 days)  | 5                           |
| **Total PRs tracked**     | **~35 unique**              |

## Edge Cases & Quirks

1. **Bot reviews (cursor, graphite-app)**: `latestReviews` includes bot accounts. Must filter by known bot patterns or ignore `COMMENTED` state from non-human reviewers when determining if the user has reviewed.

2. **Team review requests**: `reviewRequests` can contain `Team` objects (e.g., `team-eng-dealership`). The user might be part of the team. The `review-requested:@me` search handles this — GitHub resolves team membership server-side.

3. **`reviewDecision` can be null**: Happens in repos without branch protection rules or without required reviews. Fall back to manual inspection of `latestReviews`.

4. **Very old PRs**: Some PRs are years old (e.g., from 2022-2023). Consider adding a staleness cutoff to avoid clutter.

5. **`mergeable` field**: Can be `MERGEABLE`, `CONFLICTING`, or `UNKNOWN`. Often `UNKNOWN` on first query — GitHub computes it lazily. Not reliable enough to use as a primary signal without a follow-up.

6. **Overlap between search queries**: A PR where I'm the author AND have a review request from myself won't happen in practice, but PRs from `reviewedByMe` could also appear in `needsReview` if review was re-requested. The `-review-requested:@me` filter on `reviewedByMe` prevents this.

7. **`gh` CLI not authenticated**: Returns exit code 1 with `"not logged in"` message. Must detect and handle.

8. **Network failures**: `gh` returns exit code 1 with connection error. Must detect.

## Decisions Made

- **Single query approach**: One GraphQL call with 4 aliased `search` operations. Simple, efficient, 5 points/call.
- **`latestReviews` over `reviews`**: `latestReviews` is deduplicated per reviewer — much cleaner than parsing the full `reviews` timeline.
- **Merged window**: Configurable, default 7 days. Query uses `merged:>{date}` filter.
- **Pagination**: `first: 50` should cover most users. Can add cursor-based pagination later if needed.
- **No `$login` variable needed**: `@me` works in all search queries, no need to pass the username.
