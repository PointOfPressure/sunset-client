//! Bounded native collision bridge; also usable with SceneCollisions supplied by Game.
use std::io;
use horizon_core::{geometry::{Aabb, Vec3, VoxelShape}, position::{BlockPos, ChunkPos}};
use horizon_world::{
    block_collision_scan::{BlockCollisionWorld, CollisionBlock},
    collision_context::CollisionContext, entity_obstruction::EntityIdentity,
    world_movement::{EntityMovementWorld, MovementRequest, resolve_world_movement},
};
use super::sunset_lead_policy::{CollisionKernel, Error};

/// Native scans normally skip unavailable chunks. Forecasts must fail instead.
/// The wrapped SceneCollisions retains native contextual block shapes.
pub struct StrictChunks<W>(pub W);
impl<W: BlockCollisionWorld<Error=io::Error>> BlockCollisionWorld for StrictChunks<W> {
    type Error=io::Error;
    type Chunk=W::Chunk;
    fn collision_chunk(&mut self, p:ChunkPos)->io::Result<Option<W::Chunk>> {
        self.0.collision_chunk(p)?.map(Some).ok_or_else(||
            io::Error::new(io::ErrorKind::NotFound,"forecast collision chunk unavailable"))
    }
    fn collision_block(&mut self,c:W::Chunk,p:BlockPos)->io::Result<CollisionBlock>{self.0.collision_block(c,p)}
    fn is_suffocating(&mut self,c:W::Chunk,b:CollisionBlock,p:BlockPos)->io::Result<bool>{self.0.is_suffocating(c,b,p)}
    fn may_reach_out(&mut self,c:W::Chunk,y:i32)->io::Result<bool>{self.0.may_reach_out(c,y)}
    fn context_shape(&mut self,b:CollisionBlock,p:BlockPos,c:&CollisionContext<'_>)->io::Result<VoxelShape>{self.0.context_shape(b,p,c)}
}
impl<W: EntityMovementWorld<Error=io::Error>> EntityMovementWorld for StrictChunks<W> {
    type Error=io::Error;
    // Reference static forecast supplies no extra entity collision shapes.
    fn entity_collisions(&mut self,_:EntityIdentity,_:Aabb)->io::Result<Vec<VoxelShape>> { Ok(Vec::new()) }
    fn movement_border(&mut self,id:EntityIdentity,b:Aabb)->io::Result<Option<VoxelShape>>{self.0.movement_border(id,b)}
}
pub struct NativeKernel<'a,W> {
    pub world: &'a mut StrictChunks<W>,
    pub identity: EntityIdentity,
    /// Capture from the real entity, including descending/mainhand. Do not substitute
    /// a placement context or change held-item facts to the planned attack hand.
    pub context: CollisionContext<'a>,
    pub cell_budget_per_step: usize,
}
impl<W> CollisionKernel for NativeKernel<'_,W>
where W:BlockCollisionWorld<Error=io::Error>+EntityMovementWorld<Error=io::Error> {
    fn clip(&mut self,bounds:Aabb,attempted:Vec3)->Result<Vec3,Error> {
        resolve_world_movement(self.world,MovementRequest {
            entity:self.identity,context:self.context,bounds,movement:attempted,
            max_step:0.0,on_ground:false,candidate_budget:self.cell_budget_per_step.min(1024),
        }).map_err(|_|Error::Collision)
    }
}
