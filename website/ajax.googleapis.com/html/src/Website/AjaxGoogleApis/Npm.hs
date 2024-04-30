{-# LANGUAGE GeneralisedNewtypeDeriving #-}
{-# LANGUAGE LambdaCase #-}

module Website.AjaxGoogleApis.Npm (NpmPackage (..), NpmPkgSpec (..), addNpmPackage) where

import Data.Aeson
import qualified Data.Map.Strict as M
import Data.Maybe
import Data.SemVer
import Data.Text (Text)
import GHC.Stack (HasCallStack)
import Website.AjaxGoogleApis.Types

newtype NpmPackage = NpmPkg {npmVersions :: [VersionInfo]}
  deriving (Show, ToJSON)

newtype NpmPkgSpec = NpmPkgSpec (M.Map Text NpmPackage)
  deriving (Show, ToJSON)

shakaPlayer :: NpmPackage
shakaPlayer =
  NpmPkg
    { npmVersions =
        let mkInfo v = VersionInfo v (Just "dist")
            versionRange maj mi =
              fmap (\v -> mkInfo (toText (Version maj mi v [] [])))
         in concat
              [ [VersionInfo "1.6.5" Nothing],
                versionRange 2 0 [6 .. 9],
                versionRange 2 1 [0 .. 9],
                versionRange 2 2 [0 .. 10],
                versionRange 2 3 [0 .. 10],
                versionRange 2 4 [0 .. 7],
                mkInfo
                  <$> [ "2.5.0-beta",
                        "2.5.0-beta2",
                        "2.5.0-beta3"
                      ],
                versionRange 2 5 [0 .. 23],
                versionRange 3 0 [0 .. 15],
                versionRange 3 1 [0 .. 8],
                versionRange 3 2 [0 .. 21],
                versionRange 3 3 [0 .. 19],
                versionRange 4 0 [0 .. 5],
                versionRange 4 1 [0 .. 7],
                versionRange 4 2 [0 .. 14],
                versionRange 4 3 [0 .. 15],
                versionRange 4 4 [0 .. 3],
                [mkInfo "4.5.0"],
                versionRange 4 6 [0 .. 17],
                versionRange 4 7 [0 .. 13]
              ]
    }

withVerDir :: (HasCallStack) => (Version -> Maybe Text) -> [Text] -> NpmPackage
withVerDir f vs =
  NpmPkg
    ( fmap
        ( \vt -> case parseVersion vt of
            Right v -> VersionInfo {viVersion = vt, viDir = f v}
            Left _ -> error ("invalid version: " ++ show vt)
        )
        vs
    )

fixedVerDir :: Maybe Text -> [Text] -> NpmPackage
fixedVerDir d vs = NpmPkg (fmap (\vt -> VersionInfo vt d) vs)

addNpmPackage :: Package -> NpmPkgSpec -> NpmPkgSpec
addNpmPackage pkg@Package {pkgVersions = mvs} sp@(NpmPkgSpec m) =
  case pkgId pkg of
    PCesium ->
      NpmPkgSpec
        ( M.insert
            "cesium"
            (NpmPkg [VersionInfo {viVersion = "1.78.0", viDir = Nothing}])
            m
        )
    PD3 ->
      NpmPkgSpec
        ( M.insert
            "d3"
            ( withVerDir
                ( \v -> case vMajor v of
                    0 -> Nothing
                    1 -> Nothing
                    2 -> Nothing
                    3 -> Nothing
                    4 -> Just "build"
                    _ -> Just "dist"
                )
                (fromJust mvs)
            )
            m
        )
    PHammer ->
      NpmPkgSpec
        ( M.insert
            "hammerjs"
            ( withVerDir
                ( \case
                    Version 1 0 p _ _ | p <= 5 -> Just "dist"
                    _ -> Nothing
                )
                (fromJust mvs)
            )
            m
        )
    PIndefiniteObservable ->
      NpmPkgSpec
        ( M.insert
            "indefinite-observable"
            (fixedVerDir (Just "dist") (fromJust mvs))
            m
        )
    PList ->
      NpmPkgSpec
        ( M.insert
            "list.js"
            (fixedVerDir (Just "dist") (fromJust mvs))
            m
        )
    PMaterialMotion ->
      NpmPkgSpec
        ( M.insert
            "material-motion"
            (fixedVerDir (Just "dist") (fromJust mvs))
            m
        )
    PModelViewer ->
      NpmPkgSpec
        ( M.insert
            "@google/model-viewer"
            (fixedVerDir (Just "dist") (fromJust mvs))
            m
        )
    PShakaPlayer -> NpmPkgSpec (M.insert "shaka-player" shakaPlayer m)
    PSpf ->
      NpmPkgSpec
        ( M.insert
            "spf"
            (fixedVerDir (Just "dist") (fromJust mvs))
            m
        )
    PWebFontLoader ->
      NpmPkgSpec
        ( M.insert
            "webfontloader"
            (fixedVerDir Nothing (fromJust mvs))
            m
        )
    _ -> sp
