use hexlog::SemanticId;

/// Cada par `(from, to)` diz que `from` roda depois de `to`.
pub fn order(_ids: &[SemanticId], _after: &[(SemanticId, SemanticId)]) -> Vec<SemanticId> {
    unimplemented!()
}
