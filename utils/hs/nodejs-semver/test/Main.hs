{-# LANGUAGE BlockArguments #-}
{-# LANGUAGE OverloadedStrings #-}

module Main (main) where

import Data.SemVer
import Data.Text (Text)
import Test.Hspec

parseRender :: Spec
parseRender = do
  test
    "trivial"
    "1.2.34+123.456"
    Version
      { vMajor = 1,
        vMinor = 2,
        vPatch = 34,
        vPreRelease = [],
        vBuild = [INumeric 123, INumeric 456]
      }
  test
    "with prerelease"
    "1.2.34-abc.123"
    Version
      { vMajor = 1,
        vMinor = 2,
        vPatch = 34,
        vPreRelease = [IAlphaNumeric "abc", INumeric 123],
        vBuild = []
      }
  test
    "with prerelease and build"
    "1.2.34-abc.123+1"
    Version
      { vMajor = 1,
        vMinor = 2,
        vPatch = 34,
        vPreRelease = [IAlphaNumeric "abc", INumeric 123],
        vBuild = [INumeric 1]
      }
  test
    "prerelease with numeric prefix"
    "1.0.0-rc.2-migration"
    Version
      { vMajor = 1,
        vMinor = 0,
        vPatch = 0,
        vPreRelease = [IAlphaNumeric "rc", IAlphaNumeric "2-migration"],
        vBuild = []
      }
  where
    test :: String -> Text -> Version -> Spec
    test name t v = describe name do
      it "parse" do
        parseVersion t `shouldBe` Right v
      it "render" do
        toText v `shouldBe` t

versionCmp :: Spec
versionCmp = do
  let preRel = [IAlphaNumeric "abc", INumeric 123]
  testLt
    "different major"
    (Version 1 2 34 preRel [])
    (Version 2 2 34 preRel [])
  testLt
    "diffetent minor"
    (Version 1 2 34 preRel [])
    (Version 1 3 34 preRel [])
  testLt
    "different patch"
    (Version 1 2 3 preRel [])
    (Version 1 2 4 preRel [])
  describe "different pre release" do
    let alpha = IAlphaNumeric "alpha"
        beta = IAlphaNumeric "beta"
        rc = IAlphaNumeric "rc"
    let v1Alpha = Version 1 0 0 [alpha] []
        v1Alpha2 = Version 1 0 0 [alpha, INumeric 2] []
        v1AlphaBeta = Version 1 0 0 [alpha, beta] []
        v1Beta = Version 1 0 0 [beta] []
        v1Beta2 = Version 1 0 0 [beta, INumeric 2] []
        v1Beta11 = Version 1 0 0 [beta, INumeric 11] []
        v1Rc1 = Version 1 0 0 [rc, INumeric 1] []
        v1 = Version 1 0 0 [] []
    testLt "alpha < alpha.2" v1Alpha v1Alpha2
    testLt "alpha.2 < alpla.beta" v1Alpha2 v1AlphaBeta
    testLt "alpha.beta < beta" v1AlphaBeta v1Beta
    testLt "beta < beta.2" v1Beta v1Beta2
    testLt "beta.2 < beta.11" v1Beta2 v1Beta11
    testLt "beta.11 < rc.1" v1Beta11 v1Rc1
    testLt "rc.1 < v1.0" v1Rc1 v1
  where
    testLt :: String -> Version -> Version -> Spec
    testLt name v1 v2 = describe name do
      it "less" (compare v1 v2 `shouldBe` LT)
      it "greater" (compare v2 v1 `shouldBe` GT)

main :: IO ()
main = hspec do
  describe "parse and render" parseRender
  describe "compare" versionCmp
