pub fn scan_funded_invoice_expirations(
    env: &Env,
    grace_period: u64,
    limit: Option<u32>,
) -> Result<OverdueScanResult, QuickLendXError> {
    let grace_period = resolve_grace_period(env, Some(grace_period))?;
    let funded_invoices = InvoiceStorage::get_invoices_by_status(env, InvoiceStatus::Funded);
    let total_funded = funded_invoices.len();

    if total_funded == 0 {
        set_overdue_scan_cursor(env, 0);
        return Ok(OverdueScanResult {
            overdue_count: 0,
            scanned_count: 0,
            failed_count: 0,
            total_funded: 0,
            next_cursor: 0,
        });
    }

    // Bounded scan window: clamp the requested limit, then cap to the funded snapshot size.
    let scan_limit = resolve_scan_limit(limit).min(total_funded);
    let current_timestamp = env.ledger().timestamp();
    let mut cursor = normalize_cursor(get_overdue_scan_cursor(env), total_funded);
    let start_cursor = cursor;
    let mut overdue_count = 0u32;
    let mut scanned_count = 0u32;
    let mut failed_count = 0u32;

    while scanned_count < scan_limit {
        if let Some(invoice_id) = funded_invoices.get(cursor) {
            let result = (|| {
                let invoice = InvoiceStorage::get_invoice(env, &invoice_id)
                    .ok_or(QuickLendXError::InvoiceNotFound)?;

                if invoice.is_overdue(current_timestamp) {
                    overdue_count = overdue_count.saturating_add(1);
                    let _ = crate::notifications::NotificationSystem::notify_payment_overdue(
                        env, &invoice,
                    );
                }

                if current_timestamp > invoice.checked_grace_deadline(grace_period)? {
                    let _ = invoice.check_and_handle_expiration(env, grace_period)?;
                }
                Ok(())
            })();

            if let Err(error) = result {
                failed_count = failed_count.saturating_add(1);
                emit_scan_item_failed(env, &invoice_id, error as u32);
            }
        }

        scanned_count = scanned_count.saturating_add(1);
        cursor = if cursor + 1 >= total_funded {
            0
        } else {
            cursor + 1
        };
    }

    let funded_after_scan = InvoiceStorage::get_invoices_by_status(env, InvoiceStatus::Funded);
    let funded_after_count = funded_after_scan.len();
    let removed_count = total_funded.saturating_sub(funded_after_count);
    let next_cursor = if funded_after_count == 0 {
        0
    } else {
        ((start_cursor as u64 + scanned_count as u64 - removed_count as u64)
            % funded_after_count as u64) as u32
    };
    set_overdue_scan_cursor(env, next_cursor);

    Ok(OverdueScanResult {
        overdue_count,
        scanned_count,
        failed_count,
        total_funded,
        next_cursor,
    })
}