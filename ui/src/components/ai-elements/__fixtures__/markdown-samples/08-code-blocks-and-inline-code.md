修复 `emit_turn_cost` 的死锁问题：

```rust
async fn emit_turn_cost(&self, usage: &TokenUsage) {
    let budget = state.settings.read().await.monthly_budget_usd;
    // ...
}
```

调用方相应改成 `self.emit_turn_cost(usage).await`.
