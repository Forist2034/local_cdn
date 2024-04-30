{-# LANGUAGE StrictData #-}

module Data.SemVer
  ( Identifier (..),
    Version (..),
    versionParser,
    parseVersion,
    toBuilder,
    toLazyText,
    toText,
    toString,
  )
where

import Data.Attoparsec.Text
import Data.Char
import Data.Functor
import Data.Text (Text)
import qualified Data.Text.Lazy as LT
import qualified Data.Text.Lazy.Builder as LTB
import qualified Data.Text.Lazy.Builder.Int as LTB

data Identifier
  = INumeric Word
  | IAlphaNumeric Text
  deriving (Show, Eq, Ord)

data Version = Version
  { vMajor :: Word,
    vMinor :: Word,
    vPatch :: Word,
    vPreRelease :: [Identifier],
    vBuild :: [Identifier]
  }
  deriving (Show, Eq)

instance Ord Version where
  compare (Version ma0 mi0 pa0 pr0 b0) (Version ma1 mi1 pa1 pr1 b1) =
    case compare ma0 ma1 of
      EQ -> case compare mi0 mi1 of
        EQ -> case compare pa0 pa1 of
          EQ -> case (pr0, pr1) of
            ([], []) -> compare b0 b1
            ([], _ : _) -> GT
            (_ : _, []) -> LT
            (l, r) -> case compare l r of
              EQ -> compare b0 b1
              c -> c
          c -> c
        c -> c
      c -> c

identP :: Parser Identifier
identP = do
  t <- takeWhile1 (\c -> isAlphaNum c || c == '-')
  case parseOnly (decimal <* endOfInput) t of
    Right r -> pure (INumeric r)
    Left _ -> pure (IAlphaNumeric t)

versionParser :: Parser Version
versionParser = do
  ma <- decimal
  void (char '.')
  mi <- decimal
  void (char '.')
  pa <- decimal
  pre <- option [] (char '-' >> sepBy identP (char '.'))
  b <- option [] (char '+' >> sepBy identP (char '.'))
  pure
    Version
      { vMajor = ma,
        vMinor = mi,
        vPatch = pa,
        vPreRelease = pre,
        vBuild = b
      }

parseVersion :: Text -> Either String Version
parseVersion = parseOnly (versionParser <* endOfInput)

toBuilder :: Version -> LTB.Builder
toBuilder v =
  mconcat
    [ LTB.decimal (vMajor v),
      LTB.singleton '.',
      LTB.decimal (vMinor v),
      LTB.singleton '.',
      LTB.decimal (vPatch v),
      case vPreRelease v of
        [] -> mempty
        h : t -> LTB.singleton '-' <> identSep h t,
      case vBuild v of
        [] -> mempty
        h : t -> LTB.singleton '+' <> identSep h t
    ]
  where
    ident (INumeric n) = LTB.decimal n
    ident (IAlphaNumeric t) = LTB.fromText t

    identSep h t = ident h <> foldMap (\i -> LTB.singleton '.' <> ident i) t

toLazyText :: Version -> LT.Text
toLazyText v = LTB.toLazyText (toBuilder v)

toText :: Version -> Text
toText v = LT.toStrict (toLazyText v)

toString :: Version -> String
toString v = LT.unpack (toLazyText v)