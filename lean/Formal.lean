import Formal.State
import Formal.Bootstrap
import Formal.ExcResume
import Formal.Derive
import Formal.Forest
import Formal.KernelHandle
import Formal.KernelHandleTable
import Formal.Location

-- BLK-001 入口：声明模型目标清单；import 各模型模块使 lake build 覆盖 Formal.*。

def smokeState : Formal.State := { caps := fun _ => none, booted := false }

#eval smokeState        -- { caps := <function>, booted := false }
#eval Formal.bootedState.booted  -- true
