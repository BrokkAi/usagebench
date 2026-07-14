package precision

import Tools.{choose => select}
import Extensions.*

object Consumer:
  val selected = select("value")
  val decorated = "value".decorate
  val made = Maker("value")
