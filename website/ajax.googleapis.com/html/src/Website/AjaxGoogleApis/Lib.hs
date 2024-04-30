module Website.AjaxGoogleApis.Lib where

import Data.Foldable
import Data.Maybe (fromJust)
import Data.Text (Text)
import qualified Data.Text as T
import Website.AjaxGoogleApis.Npm
import Website.AjaxGoogleApis.Types

data Libraries = Libraries
  { libNpm :: NpmPkgSpec,
    libThree :: [VersionInfo],
    libJQuery :: [Text],
    -- TODO:
    -- libJQueryUI :: [VersionInfo],
    -- libJQueryMobile :: [VersionInfo],
    libDojo :: [Text]
  }

toLibraries :: [Package] -> Libraries
toLibraries =
  foldl'
    ( \lib p@Package {pkgVersions = mvs} -> case pkgId p of
        PDojo -> lib {libDojo = fromJust mvs}
        PExtCore -> lib
        PJQuery -> lib {libJQuery = fromJust mvs}
        -- PJQueryUI -> _
        -- PJQueryMobile -> _
        PThree ->
          lib
            { libThree =
                fmap
                  ( \vt ->
                      let v = read (T.unpack (T.tail vt)) :: Word
                       in VersionInfo vt (if v < 77 then Nothing else Just "build")
                  )
                  (fromJust mvs)
            }
        _ -> lib {libNpm = addNpmPackage p (libNpm lib)}
    )
    Libraries
      { libNpm = NpmPkgSpec mempty,
        libThree = [],
        libJQuery = [],
        -- libJQueryUI = [],
        -- libJQueryMobile = [],
        libDojo = []
      }
