# QuickLendX - Invoice Rating System

The Invoice Rating System allows investors who have funded an invoice to provide feedback and a star rating (1-5) on their experience. This enhances the protocol's trust layer, giving businesses a verifiable track record of successful transactions.

## Core Features

- **Secured Permissions**: Only the exact `investor` address that funded the invoice is authorized to leave a rating.
- **Valid States**: Ratings can only be submitted if the `InvoiceStatus` is `Funded` or `Paid`.
- **Single Vote**: An investor may only rate a given invoice once. Future updates can be handled natively but the initial lock prevents spam.
- **Aggregation Metrics**: The protocol handles ongoing tallying of `average_rating`, `total_ratings`, `highest_rating`, and `lowest_rating` safely.

## Key Methods

### `add_rating(rating: u32, feedback: String, rater: Address, timestamp: u64)`
Attaches a rating (1-5) and feedback description. 
**Reverts with**:
- `NotFunded` if the invoice isn't Funded or Paid.
- `NotRater` if the sender is not the funder.
- `InvalidRating` if the score is `< 1` or `> 5`.
- `AlreadyRated` if the investor previously left feedback.

### `get_invoice_rating_stats() -> InvoiceRatingStats`
Returns a unified `InvoiceRatingStats` object detailing:
- `average_rating`
- `total_ratings`
- `highest_rating`
- `lowest_rating`

### `InvoiceStorage::get_invoices_with_rating_above(env: &Env, threshold: u32) -> Vec<BytesN<32>>`
Fetches a list of invoice IDs that hold an average score equal to or exceeding the provided `threshold`. Extremely helpful for building premium front-end explorer tabs.