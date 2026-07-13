//! Prozessuebergreifende Serialisierung aller schreibenden Proposal-Aktionen.

use sqlx::{Postgres, Transaction};
use turnier_db::Pool;

use crate::error::WebResult;

const LOCK_NAMESPACE: i64 = 0x5455_524E_0000_0000;

/// Die offene Transaktion haelt den Advisory Lock bis zum Ende der Aktion.
pub(crate) struct ProposalLock {
    _transaction: Transaction<'static, Postgres>,
}

pub(crate) async fn acquire(pool: &Pool, proposal_id: i64) -> WebResult<ProposalLock> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(LOCK_NAMESPACE ^ proposal_id)
        .execute(&mut *transaction)
        .await?;
    Ok(ProposalLock {
        _transaction: transaction,
    })
}

#[cfg(all(test, feature = "testing"))]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn gleicher_vorschlag_wird_serialisiert() {
        let db = turnier_db::test_pool().await.expect("central test pool");
        let first = acquire(db.pool(), 42).await.unwrap();

        assert!(tokio::time::timeout(
            Duration::from_millis(100),
            acquire(db.pool(), 42)
        )
        .await
        .is_err());
        acquire(db.pool(), 43).await.unwrap();

        drop(first);
        tokio::time::timeout(Duration::from_secs(1), acquire(db.pool(), 42))
            .await
            .expect("Lock wurde freigegeben")
            .unwrap();
    }
}
