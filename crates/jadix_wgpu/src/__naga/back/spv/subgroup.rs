use super::{Block, BlockContext, Error, Instruction, NumericType};
use crate::__naga::{arena::Handle, TypeInner};

impl BlockContext<'_> {
    pub(super) fn write_subgroup_ballot(
        &mut self,
        predicate: &Option<Handle<crate::__naga::Expression>>,
        result: Handle<crate::__naga::Expression>,
        block: &mut Block,
    ) -> Result<(), Error> {
        self.writer.require_any(
            "GroupNonUniformBallot",
            &[spirv::Capability::GroupNonUniformBallot],
        )?;
        let vec4_u32_type_id = self.get_numeric_type_id(NumericType::Vector {
            size: crate::__naga::VectorSize::Quad,
            scalar: crate::__naga::Scalar::U32,
        });
        let exec_scope_id = self.get_index_constant(spirv::Scope::Subgroup as u32);
        let predicate = if let Some(predicate) = *predicate {
            self.cached[predicate]
        } else {
            self.writer
                .get_constant_scalar(crate::__naga::Literal::Bool(true))
        };
        let id = self.gen_id();
        block.body.push(Instruction::group_non_uniform_ballot(
            vec4_u32_type_id,
            id,
            exec_scope_id,
            predicate,
        ));
        self.cached[result] = id;
        Ok(())
    }
    pub(super) fn write_subgroup_operation(
        &mut self,
        op: &crate::__naga::SubgroupOperation,
        collective_op: &crate::__naga::CollectiveOperation,
        argument: Handle<crate::__naga::Expression>,
        result: Handle<crate::__naga::Expression>,
        block: &mut Block,
    ) -> Result<(), Error> {
        use crate::__naga::SubgroupOperation as sg;
        match *op {
            sg::All | sg::Any => {
                self.writer.require_any(
                    "GroupNonUniformVote",
                    &[spirv::Capability::GroupNonUniformVote],
                )?;
            }
            _ => {
                self.writer.require_any(
                    "GroupNonUniformArithmetic",
                    &[spirv::Capability::GroupNonUniformArithmetic],
                )?;
            }
        }

        let id = self.gen_id();
        let result_ty = &self.fun_info[result].ty;
        let result_type_id = self.get_expression_type_id(result_ty);
        let result_ty_inner = result_ty.inner_with(&self.ir_module.types);

        let (is_scalar, scalar) = match *result_ty_inner {
            TypeInner::Scalar(kind) => (true, kind),
            TypeInner::Vector { scalar: kind, .. } => (false, kind),
            _ => unimplemented!(),
        };

        use crate::__naga::ScalarKind as sk;
        let spirv_op = match (scalar.kind, *op) {
            (sk::Bool, sg::All) if is_scalar => spirv::Op::GroupNonUniformAll,
            (sk::Bool, sg::Any) if is_scalar => spirv::Op::GroupNonUniformAny,
            (_, sg::All | sg::Any) => unimplemented!(),

            (sk::Sint | sk::Uint, sg::Add) => spirv::Op::GroupNonUniformIAdd,
            (sk::Float, sg::Add) => spirv::Op::GroupNonUniformFAdd,
            (sk::Sint | sk::Uint, sg::Mul) => spirv::Op::GroupNonUniformIMul,
            (sk::Float, sg::Mul) => spirv::Op::GroupNonUniformFMul,
            (sk::Sint, sg::Max) => spirv::Op::GroupNonUniformSMax,
            (sk::Uint, sg::Max) => spirv::Op::GroupNonUniformUMax,
            (sk::Float, sg::Max) => spirv::Op::GroupNonUniformFMax,
            (sk::Sint, sg::Min) => spirv::Op::GroupNonUniformSMin,
            (sk::Uint, sg::Min) => spirv::Op::GroupNonUniformUMin,
            (sk::Float, sg::Min) => spirv::Op::GroupNonUniformFMin,
            (_, sg::Add | sg::Mul | sg::Min | sg::Max) => unimplemented!(),

            (sk::Sint | sk::Uint, sg::And) => spirv::Op::GroupNonUniformBitwiseAnd,
            (sk::Sint | sk::Uint, sg::Or) => spirv::Op::GroupNonUniformBitwiseOr,
            (sk::Sint | sk::Uint, sg::Xor) => spirv::Op::GroupNonUniformBitwiseXor,
            (sk::Bool, sg::And) => spirv::Op::GroupNonUniformLogicalAnd,
            (sk::Bool, sg::Or) => spirv::Op::GroupNonUniformLogicalOr,
            (sk::Bool, sg::Xor) => spirv::Op::GroupNonUniformLogicalXor,
            (_, sg::And | sg::Or | sg::Xor) => unimplemented!(),
        };

        let exec_scope_id = self.get_index_constant(spirv::Scope::Subgroup as u32);

        use crate::__naga::CollectiveOperation as c;
        let group_op = match *op {
            sg::All | sg::Any => None,
            _ => Some(match *collective_op {
                c::Reduce => spirv::GroupOperation::Reduce,
                c::InclusiveScan => spirv::GroupOperation::InclusiveScan,
                c::ExclusiveScan => spirv::GroupOperation::ExclusiveScan,
            }),
        };

        let arg_id = self.cached[argument];
        block.body.push(Instruction::group_non_uniform_arithmetic(
            spirv_op,
            result_type_id,
            id,
            exec_scope_id,
            group_op,
            arg_id,
        ));
        self.cached[result] = id;
        Ok(())
    }
    pub(super) fn write_subgroup_gather(
        &mut self,
        mode: &crate::__naga::GatherMode,
        argument: Handle<crate::__naga::Expression>,
        result: Handle<crate::__naga::Expression>,
        block: &mut Block,
    ) -> Result<(), Error> {
        match *mode {
            crate::__naga::GatherMode::BroadcastFirst => {
                self.writer.require_any(
                    "GroupNonUniformBallot",
                    &[spirv::Capability::GroupNonUniformBallot],
                )?;
            }
            crate::__naga::GatherMode::Shuffle(_)
            | crate::__naga::GatherMode::ShuffleXor(_)
            | crate::__naga::GatherMode::Broadcast(_) => {
                self.writer.require_any(
                    "GroupNonUniformShuffle",
                    &[spirv::Capability::GroupNonUniformShuffle],
                )?;
            }
            crate::__naga::GatherMode::ShuffleDown(_) | crate::__naga::GatherMode::ShuffleUp(_) => {
                self.writer.require_any(
                    "GroupNonUniformShuffleRelative",
                    &[spirv::Capability::GroupNonUniformShuffleRelative],
                )?;
            }
            crate::__naga::GatherMode::QuadBroadcast(_)
            | crate::__naga::GatherMode::QuadSwap(_) => {
                self.writer.require_any(
                    "GroupNonUniformQuad",
                    &[spirv::Capability::GroupNonUniformQuad],
                )?;
            }
        }

        let id = self.gen_id();
        let result_ty = &self.fun_info[result].ty;
        let result_type_id = self.get_expression_type_id(result_ty);

        let exec_scope_id = self.get_index_constant(spirv::Scope::Subgroup as u32);

        let arg_id = self.cached[argument];
        match *mode {
            crate::__naga::GatherMode::BroadcastFirst => {
                block
                    .body
                    .push(Instruction::group_non_uniform_broadcast_first(
                        result_type_id,
                        id,
                        exec_scope_id,
                        arg_id,
                    ));
            }
            crate::__naga::GatherMode::Broadcast(index)
            | crate::__naga::GatherMode::Shuffle(index)
            | crate::__naga::GatherMode::ShuffleDown(index)
            | crate::__naga::GatherMode::ShuffleUp(index)
            | crate::__naga::GatherMode::ShuffleXor(index)
            | crate::__naga::GatherMode::QuadBroadcast(index) => {
                let index_id = self.cached[index];
                let op = match *mode {
                    crate::__naga::GatherMode::BroadcastFirst => unreachable!(),
                    // Use shuffle to emit broadcast to allow the index to
                    // be dynamically uniform on Vulkan 1.1. The argument to
                    // OpGroupNonUniformBroadcast must be a constant pre SPIR-V
                    // 1.5 (vulkan 1.2)
                    crate::__naga::GatherMode::Broadcast(_) => spirv::Op::GroupNonUniformShuffle,
                    crate::__naga::GatherMode::Shuffle(_) => spirv::Op::GroupNonUniformShuffle,
                    crate::__naga::GatherMode::ShuffleDown(_) => {
                        spirv::Op::GroupNonUniformShuffleDown
                    }
                    crate::__naga::GatherMode::ShuffleUp(_) => spirv::Op::GroupNonUniformShuffleUp,
                    crate::__naga::GatherMode::ShuffleXor(_) => {
                        spirv::Op::GroupNonUniformShuffleXor
                    }
                    crate::__naga::GatherMode::QuadBroadcast(_) => {
                        spirv::Op::GroupNonUniformQuadBroadcast
                    }
                    crate::__naga::GatherMode::QuadSwap(_) => unreachable!(),
                };
                block.body.push(Instruction::group_non_uniform_gather(
                    op,
                    result_type_id,
                    id,
                    exec_scope_id,
                    arg_id,
                    index_id,
                ));
            }
            crate::__naga::GatherMode::QuadSwap(direction) => {
                let direction = self.get_index_constant(match direction {
                    crate::__naga::Direction::X => 0,
                    crate::__naga::Direction::Y => 1,
                    crate::__naga::Direction::Diagonal => 2,
                });
                block.body.push(Instruction::group_non_uniform_quad_swap(
                    result_type_id,
                    id,
                    exec_scope_id,
                    arg_id,
                    direction,
                ));
            }
        }
        self.cached[result] = id;
        Ok(())
    }
}
