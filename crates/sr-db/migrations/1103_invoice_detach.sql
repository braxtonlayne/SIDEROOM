-- 1103: N-08 (D-325): after the invoice retention period the invoice stays (amounts, dates, state, the chain
-- address) but no longer points at the member. The monthly job `payments.detach_expired_invoices` sets
-- `user_id` to NULL; readers treat NULL as "detached". The foreign key stays.
ALTER TABLE payment_invoices ALTER COLUMN user_id DROP NOT NULL;
