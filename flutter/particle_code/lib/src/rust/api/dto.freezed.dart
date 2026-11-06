// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'dto.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$LivenessDto {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is LivenessDto);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'LivenessDto()';
}


}

/// @nodoc
class $LivenessDtoCopyWith<$Res>  {
$LivenessDtoCopyWith(LivenessDto _, $Res Function(LivenessDto) __);
}


/// Adds pattern-matching-related methods to [LivenessDto].
extension LivenessDtoPatterns on LivenessDto {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( LivenessDto_Unknown value)?  unknown,TResult Function( LivenessDto_Read value)?  read,required TResult orElse(),}){
final _that = this;
switch (_that) {
case LivenessDto_Unknown() when unknown != null:
return unknown(_that);case LivenessDto_Read() when read != null:
return read(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( LivenessDto_Unknown value)  unknown,required TResult Function( LivenessDto_Read value)  read,}){
final _that = this;
switch (_that) {
case LivenessDto_Unknown():
return unknown(_that);case LivenessDto_Read():
return read(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( LivenessDto_Unknown value)?  unknown,TResult? Function( LivenessDto_Read value)?  read,}){
final _that = this;
switch (_that) {
case LivenessDto_Unknown() when unknown != null:
return unknown(_that);case LivenessDto_Read() when read != null:
return read(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  unknown,TResult Function( int seedLow,  int rolling,  bool consistent)?  read,required TResult orElse(),}) {final _that = this;
switch (_that) {
case LivenessDto_Unknown() when unknown != null:
return unknown();case LivenessDto_Read() when read != null:
return read(_that.seedLow,_that.rolling,_that.consistent);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  unknown,required TResult Function( int seedLow,  int rolling,  bool consistent)  read,}) {final _that = this;
switch (_that) {
case LivenessDto_Unknown():
return unknown();case LivenessDto_Read():
return read(_that.seedLow,_that.rolling,_that.consistent);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  unknown,TResult? Function( int seedLow,  int rolling,  bool consistent)?  read,}) {final _that = this;
switch (_that) {
case LivenessDto_Unknown() when unknown != null:
return unknown();case LivenessDto_Read() when read != null:
return read(_that.seedLow,_that.rolling,_that.consistent);case _:
  return null;

}
}

}

/// @nodoc


class LivenessDto_Unknown extends LivenessDto {
  const LivenessDto_Unknown(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is LivenessDto_Unknown);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'LivenessDto.unknown()';
}


}




/// @nodoc


class LivenessDto_Read extends LivenessDto {
  const LivenessDto_Read({required this.seedLow, required this.rolling, required this.consistent}): super._();
  

/// Low byte of the packet seed, as shown by the particles.
 final  int seedLow;
/// The 16-bit rolling HMAC value, as shown by the particles; check it with
/// `liveness_verify`.
 final  int rolling;
/// Whether `seed_low` matches the decoded header's packet seed.
 final  bool consistent;

/// Create a copy of LivenessDto
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$LivenessDto_ReadCopyWith<LivenessDto_Read> get copyWith => _$LivenessDto_ReadCopyWithImpl<LivenessDto_Read>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is LivenessDto_Read&&(identical(other.seedLow, seedLow) || other.seedLow == seedLow)&&(identical(other.rolling, rolling) || other.rolling == rolling)&&(identical(other.consistent, consistent) || other.consistent == consistent));
}


@override
int get hashCode {
    return Object.hash(runtimeType,seedLow,rolling,consistent);
}

@override
String toString() {
    return 'LivenessDto.read(seedLow: $seedLow, rolling: $rolling, consistent: $consistent)';
}


}

/// @nodoc
abstract mixin class $LivenessDto_ReadCopyWith<$Res> implements $LivenessDtoCopyWith<$Res> {
  factory $LivenessDto_ReadCopyWith(LivenessDto_Read value, $Res Function(LivenessDto_Read) _then) = _$LivenessDto_ReadCopyWithImpl;
@useResult
$Res call({
 int seedLow, int rolling, bool consistent
});




}
/// @nodoc
class _$LivenessDto_ReadCopyWithImpl<$Res>
    implements $LivenessDto_ReadCopyWith<$Res> {
  _$LivenessDto_ReadCopyWithImpl(this._self, this._then);

  final LivenessDto_Read _self;
  final $Res Function(LivenessDto_Read) _then;

/// Create a copy of LivenessDto
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? seedLow = null,Object? rolling = null,Object? consistent = null,}) {
  return _then(LivenessDto_Read(
seedLow: null == seedLow ? _self.seedLow : seedLow // ignore: cast_nullable_to_non_nullable
as int,rolling: null == rolling ? _self.rolling : rolling // ignore: cast_nullable_to_non_nullable
as int,consistent: null == consistent ? _self.consistent : consistent // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

/// @nodoc
mixin _$ProgressDto {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is ProgressDto);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'ProgressDto()';
}


}

/// @nodoc
class $ProgressDtoCopyWith<$Res>  {
$ProgressDtoCopyWith(ProgressDto _, $Res Function(ProgressDto) __);
}


/// Adds pattern-matching-related methods to [ProgressDto].
extension ProgressDtoPatterns on ProgressDto {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( ProgressDto_Rejected value)?  rejected,TResult Function( ProgressDto_Accepted value)?  accepted,TResult Function( ProgressDto_Done value)?  done,required TResult orElse(),}){
final _that = this;
switch (_that) {
case ProgressDto_Rejected() when rejected != null:
return rejected(_that);case ProgressDto_Accepted() when accepted != null:
return accepted(_that);case ProgressDto_Done() when done != null:
return done(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( ProgressDto_Rejected value)  rejected,required TResult Function( ProgressDto_Accepted value)  accepted,required TResult Function( ProgressDto_Done value)  done,}){
final _that = this;
switch (_that) {
case ProgressDto_Rejected():
return rejected(_that);case ProgressDto_Accepted():
return accepted(_that);case ProgressDto_Done():
return done(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( ProgressDto_Rejected value)?  rejected,TResult? Function( ProgressDto_Accepted value)?  accepted,TResult? Function( ProgressDto_Done value)?  done,}){
final _that = this;
switch (_that) {
case ProgressDto_Rejected() when rejected != null:
return rejected(_that);case ProgressDto_Accepted() when accepted != null:
return accepted(_that);case ProgressDto_Done() when done != null:
return done(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String reason)?  rejected,TResult Function( int have,  int need,  int segmentsDone,  int segmentsTotal)?  accepted,TResult Function( Uint8List payload)?  done,required TResult orElse(),}) {final _that = this;
switch (_that) {
case ProgressDto_Rejected() when rejected != null:
return rejected(_that.reason);case ProgressDto_Accepted() when accepted != null:
return accepted(_that.have,_that.need,_that.segmentsDone,_that.segmentsTotal);case ProgressDto_Done() when done != null:
return done(_that.payload);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String reason)  rejected,required TResult Function( int have,  int need,  int segmentsDone,  int segmentsTotal)  accepted,required TResult Function( Uint8List payload)  done,}) {final _that = this;
switch (_that) {
case ProgressDto_Rejected():
return rejected(_that.reason);case ProgressDto_Accepted():
return accepted(_that.have,_that.need,_that.segmentsDone,_that.segmentsTotal);case ProgressDto_Done():
return done(_that.payload);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String reason)?  rejected,TResult? Function( int have,  int need,  int segmentsDone,  int segmentsTotal)?  accepted,TResult? Function( Uint8List payload)?  done,}) {final _that = this;
switch (_that) {
case ProgressDto_Rejected() when rejected != null:
return rejected(_that.reason);case ProgressDto_Accepted() when accepted != null:
return accepted(_that.have,_that.need,_that.segmentsDone,_that.segmentsTotal);case ProgressDto_Done() when done != null:
return done(_that.payload);case _:
  return null;

}
}

}

/// @nodoc


class ProgressDto_Rejected extends ProgressDto {
  const ProgressDto_Rejected({required this.reason}): super._();
  

/// Why, in words.
 final  String reason;

/// Create a copy of ProgressDto
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ProgressDto_RejectedCopyWith<ProgressDto_Rejected> get copyWith => _$ProgressDto_RejectedCopyWithImpl<ProgressDto_Rejected>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is ProgressDto_Rejected&&(identical(other.reason, reason) || other.reason == reason));
}


@override
int get hashCode {
    return Object.hash(runtimeType,reason);
}

@override
String toString() {
    return 'ProgressDto.rejected(reason: $reason)';
}


}

/// @nodoc
abstract mixin class $ProgressDto_RejectedCopyWith<$Res> implements $ProgressDtoCopyWith<$Res> {
  factory $ProgressDto_RejectedCopyWith(ProgressDto_Rejected value, $Res Function(ProgressDto_Rejected) _then) = _$ProgressDto_RejectedCopyWithImpl;
@useResult
$Res call({
 String reason
});




}
/// @nodoc
class _$ProgressDto_RejectedCopyWithImpl<$Res>
    implements $ProgressDto_RejectedCopyWith<$Res> {
  _$ProgressDto_RejectedCopyWithImpl(this._self, this._then);

  final ProgressDto_Rejected _self;
  final $Res Function(ProgressDto_Rejected) _then;

/// Create a copy of ProgressDto
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? reason = null,}) {
  return _then(ProgressDto_Rejected(
reason: null == reason ? _self.reason : reason // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class ProgressDto_Accepted extends ProgressDto {
  const ProgressDto_Accepted({required this.have, required this.need, required this.segmentsDone, required this.segmentsTotal}): super._();
  

/// Distinct symbols held, summed over segments (a decoded segment counts fully).
 final  int have;
/// Symbols needed if nothing is lost.
 final  int need;
/// Segments decoded and verified so far.
 final  int segmentsDone;
/// Segments in the transfer.
 final  int segmentsTotal;

/// Create a copy of ProgressDto
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ProgressDto_AcceptedCopyWith<ProgressDto_Accepted> get copyWith => _$ProgressDto_AcceptedCopyWithImpl<ProgressDto_Accepted>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is ProgressDto_Accepted&&(identical(other.have, have) || other.have == have)&&(identical(other.need, need) || other.need == need)&&(identical(other.segmentsDone, segmentsDone) || other.segmentsDone == segmentsDone)&&(identical(other.segmentsTotal, segmentsTotal) || other.segmentsTotal == segmentsTotal));
}


@override
int get hashCode {
    return Object.hash(runtimeType,have,need,segmentsDone,segmentsTotal);
}

@override
String toString() {
    return 'ProgressDto.accepted(have: $have, need: $need, segmentsDone: $segmentsDone, segmentsTotal: $segmentsTotal)';
}


}

/// @nodoc
abstract mixin class $ProgressDto_AcceptedCopyWith<$Res> implements $ProgressDtoCopyWith<$Res> {
  factory $ProgressDto_AcceptedCopyWith(ProgressDto_Accepted value, $Res Function(ProgressDto_Accepted) _then) = _$ProgressDto_AcceptedCopyWithImpl;
@useResult
$Res call({
 int have, int need, int segmentsDone, int segmentsTotal
});




}
/// @nodoc
class _$ProgressDto_AcceptedCopyWithImpl<$Res>
    implements $ProgressDto_AcceptedCopyWith<$Res> {
  _$ProgressDto_AcceptedCopyWithImpl(this._self, this._then);

  final ProgressDto_Accepted _self;
  final $Res Function(ProgressDto_Accepted) _then;

/// Create a copy of ProgressDto
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? have = null,Object? need = null,Object? segmentsDone = null,Object? segmentsTotal = null,}) {
  return _then(ProgressDto_Accepted(
have: null == have ? _self.have : have // ignore: cast_nullable_to_non_nullable
as int,need: null == need ? _self.need : need // ignore: cast_nullable_to_non_nullable
as int,segmentsDone: null == segmentsDone ? _self.segmentsDone : segmentsDone // ignore: cast_nullable_to_non_nullable
as int,segmentsTotal: null == segmentsTotal ? _self.segmentsTotal : segmentsTotal // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class ProgressDto_Done extends ProgressDto {
  const ProgressDto_Done({required this.payload}): super._();
  

/// The payload.
 final  Uint8List payload;

/// Create a copy of ProgressDto
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ProgressDto_DoneCopyWith<ProgressDto_Done> get copyWith => _$ProgressDto_DoneCopyWithImpl<ProgressDto_Done>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is ProgressDto_Done&&const DeepCollectionEquality().equals(other.payload, payload));
}


@override
int get hashCode {
    return Object.hash(runtimeType,const DeepCollectionEquality().hash(payload));
}

@override
String toString() {
    return 'ProgressDto.done(payload: $payload)';
}


}

/// @nodoc
abstract mixin class $ProgressDto_DoneCopyWith<$Res> implements $ProgressDtoCopyWith<$Res> {
  factory $ProgressDto_DoneCopyWith(ProgressDto_Done value, $Res Function(ProgressDto_Done) _then) = _$ProgressDto_DoneCopyWithImpl;
@useResult
$Res call({
 Uint8List payload
});




}
/// @nodoc
class _$ProgressDto_DoneCopyWithImpl<$Res>
    implements $ProgressDto_DoneCopyWith<$Res> {
  _$ProgressDto_DoneCopyWithImpl(this._self, this._then);

  final ProgressDto_Done _self;
  final $Res Function(ProgressDto_Done) _then;

/// Create a copy of ProgressDto
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? payload = null,}) {
  return _then(ProgressDto_Done(
payload: null == payload ? _self.payload : payload // ignore: cast_nullable_to_non_nullable
as Uint8List,
  ));
}


}

/// @nodoc
mixin _$ShapeChoiceDto {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is ShapeChoiceDto);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'ShapeChoiceDto()';
}


}

/// @nodoc
class $ShapeChoiceDtoCopyWith<$Res>  {
$ShapeChoiceDtoCopyWith(ShapeChoiceDto _, $Res Function(ShapeChoiceDto) __);
}


/// Adds pattern-matching-related methods to [ShapeChoiceDto].
extension ShapeChoiceDtoPatterns on ShapeChoiceDto {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( ShapeChoiceDto_Require value)?  require,TResult Function( ShapeChoiceDto_Prefer value)?  prefer,TResult Function( ShapeChoiceDto_Auto value)?  auto,required TResult orElse(),}){
final _that = this;
switch (_that) {
case ShapeChoiceDto_Require() when require != null:
return require(_that);case ShapeChoiceDto_Prefer() when prefer != null:
return prefer(_that);case ShapeChoiceDto_Auto() when auto != null:
return auto(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( ShapeChoiceDto_Require value)  require,required TResult Function( ShapeChoiceDto_Prefer value)  prefer,required TResult Function( ShapeChoiceDto_Auto value)  auto,}){
final _that = this;
switch (_that) {
case ShapeChoiceDto_Require():
return require(_that);case ShapeChoiceDto_Prefer():
return prefer(_that);case ShapeChoiceDto_Auto():
return auto(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( ShapeChoiceDto_Require value)?  require,TResult? Function( ShapeChoiceDto_Prefer value)?  prefer,TResult? Function( ShapeChoiceDto_Auto value)?  auto,}){
final _that = this;
switch (_that) {
case ShapeChoiceDto_Require() when require != null:
return require(_that);case ShapeChoiceDto_Prefer() when prefer != null:
return prefer(_that);case ShapeChoiceDto_Auto() when auto != null:
return auto(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( int id)?  require,TResult Function( int id)?  prefer,TResult Function( Uint8List ids)?  auto,required TResult orElse(),}) {final _that = this;
switch (_that) {
case ShapeChoiceDto_Require() when require != null:
return require(_that.id);case ShapeChoiceDto_Prefer() when prefer != null:
return prefer(_that.id);case ShapeChoiceDto_Auto() when auto != null:
return auto(_that.ids);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( int id)  require,required TResult Function( int id)  prefer,required TResult Function( Uint8List ids)  auto,}) {final _that = this;
switch (_that) {
case ShapeChoiceDto_Require():
return require(_that.id);case ShapeChoiceDto_Prefer():
return prefer(_that.id);case ShapeChoiceDto_Auto():
return auto(_that.ids);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( int id)?  require,TResult? Function( int id)?  prefer,TResult? Function( Uint8List ids)?  auto,}) {final _that = this;
switch (_that) {
case ShapeChoiceDto_Require() when require != null:
return require(_that.id);case ShapeChoiceDto_Prefer() when prefer != null:
return prefer(_that.id);case ShapeChoiceDto_Auto() when auto != null:
return auto(_that.ids);case _:
  return null;

}
}

}

/// @nodoc


class ShapeChoiceDto_Require extends ShapeChoiceDto {
  const ShapeChoiceDto_Require({required this.id}): super._();
  

/// Shape id.
 final  int id;

/// Create a copy of ShapeChoiceDto
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ShapeChoiceDto_RequireCopyWith<ShapeChoiceDto_Require> get copyWith => _$ShapeChoiceDto_RequireCopyWithImpl<ShapeChoiceDto_Require>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is ShapeChoiceDto_Require&&(identical(other.id, id) || other.id == id));
}


@override
int get hashCode {
    return Object.hash(runtimeType,id);
}

@override
String toString() {
    return 'ShapeChoiceDto.require(id: $id)';
}


}

/// @nodoc
abstract mixin class $ShapeChoiceDto_RequireCopyWith<$Res> implements $ShapeChoiceDtoCopyWith<$Res> {
  factory $ShapeChoiceDto_RequireCopyWith(ShapeChoiceDto_Require value, $Res Function(ShapeChoiceDto_Require) _then) = _$ShapeChoiceDto_RequireCopyWithImpl;
@useResult
$Res call({
 int id
});




}
/// @nodoc
class _$ShapeChoiceDto_RequireCopyWithImpl<$Res>
    implements $ShapeChoiceDto_RequireCopyWith<$Res> {
  _$ShapeChoiceDto_RequireCopyWithImpl(this._self, this._then);

  final ShapeChoiceDto_Require _self;
  final $Res Function(ShapeChoiceDto_Require) _then;

/// Create a copy of ShapeChoiceDto
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? id = null,}) {
  return _then(ShapeChoiceDto_Require(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class ShapeChoiceDto_Prefer extends ShapeChoiceDto {
  const ShapeChoiceDto_Prefer({required this.id}): super._();
  

/// Shape id.
 final  int id;

/// Create a copy of ShapeChoiceDto
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ShapeChoiceDto_PreferCopyWith<ShapeChoiceDto_Prefer> get copyWith => _$ShapeChoiceDto_PreferCopyWithImpl<ShapeChoiceDto_Prefer>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is ShapeChoiceDto_Prefer&&(identical(other.id, id) || other.id == id));
}


@override
int get hashCode {
    return Object.hash(runtimeType,id);
}

@override
String toString() {
    return 'ShapeChoiceDto.prefer(id: $id)';
}


}

/// @nodoc
abstract mixin class $ShapeChoiceDto_PreferCopyWith<$Res> implements $ShapeChoiceDtoCopyWith<$Res> {
  factory $ShapeChoiceDto_PreferCopyWith(ShapeChoiceDto_Prefer value, $Res Function(ShapeChoiceDto_Prefer) _then) = _$ShapeChoiceDto_PreferCopyWithImpl;
@useResult
$Res call({
 int id
});




}
/// @nodoc
class _$ShapeChoiceDto_PreferCopyWithImpl<$Res>
    implements $ShapeChoiceDto_PreferCopyWith<$Res> {
  _$ShapeChoiceDto_PreferCopyWithImpl(this._self, this._then);

  final ShapeChoiceDto_Prefer _self;
  final $Res Function(ShapeChoiceDto_Prefer) _then;

/// Create a copy of ShapeChoiceDto
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? id = null,}) {
  return _then(ShapeChoiceDto_Prefer(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class ShapeChoiceDto_Auto extends ShapeChoiceDto {
  const ShapeChoiceDto_Auto({required this.ids}): super._();
  

/// Shape ids, best first.
 final  Uint8List ids;

/// Create a copy of ShapeChoiceDto
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ShapeChoiceDto_AutoCopyWith<ShapeChoiceDto_Auto> get copyWith => _$ShapeChoiceDto_AutoCopyWithImpl<ShapeChoiceDto_Auto>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is ShapeChoiceDto_Auto&&const DeepCollectionEquality().equals(other.ids, ids));
}


@override
int get hashCode {
    return Object.hash(runtimeType,const DeepCollectionEquality().hash(ids));
}

@override
String toString() {
    return 'ShapeChoiceDto.auto(ids: $ids)';
}


}

/// @nodoc
abstract mixin class $ShapeChoiceDto_AutoCopyWith<$Res> implements $ShapeChoiceDtoCopyWith<$Res> {
  factory $ShapeChoiceDto_AutoCopyWith(ShapeChoiceDto_Auto value, $Res Function(ShapeChoiceDto_Auto) _then) = _$ShapeChoiceDto_AutoCopyWithImpl;
@useResult
$Res call({
 Uint8List ids
});




}
/// @nodoc
class _$ShapeChoiceDto_AutoCopyWithImpl<$Res>
    implements $ShapeChoiceDto_AutoCopyWith<$Res> {
  _$ShapeChoiceDto_AutoCopyWithImpl(this._self, this._then);

  final ShapeChoiceDto_Auto _self;
  final $Res Function(ShapeChoiceDto_Auto) _then;

/// Create a copy of ShapeChoiceDto
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? ids = null,}) {
  return _then(ShapeChoiceDto_Auto(
ids: null == ids ? _self.ids : ids // ignore: cast_nullable_to_non_nullable
as Uint8List,
  ));
}


}

/// @nodoc
mixin _$SwatchColorDto {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is SwatchColorDto);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'SwatchColorDto()';
}


}

/// @nodoc
class $SwatchColorDtoCopyWith<$Res>  {
$SwatchColorDtoCopyWith(SwatchColorDto _, $Res Function(SwatchColorDto) __);
}


/// Adds pattern-matching-related methods to [SwatchColorDto].
extension SwatchColorDtoPatterns on SwatchColorDto {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( SwatchColorDto_Hue value)?  hue,TResult Function( SwatchColorDto_White value)?  white,TResult Function( SwatchColorDto_Black value)?  black,required TResult orElse(),}){
final _that = this;
switch (_that) {
case SwatchColorDto_Hue() when hue != null:
return hue(_that);case SwatchColorDto_White() when white != null:
return white(_that);case SwatchColorDto_Black() when black != null:
return black(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( SwatchColorDto_Hue value)  hue,required TResult Function( SwatchColorDto_White value)  white,required TResult Function( SwatchColorDto_Black value)  black,}){
final _that = this;
switch (_that) {
case SwatchColorDto_Hue():
return hue(_that);case SwatchColorDto_White():
return white(_that);case SwatchColorDto_Black():
return black(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( SwatchColorDto_Hue value)?  hue,TResult? Function( SwatchColorDto_White value)?  white,TResult? Function( SwatchColorDto_Black value)?  black,}){
final _that = this;
switch (_that) {
case SwatchColorDto_Hue() when hue != null:
return hue(_that);case SwatchColorDto_White() when white != null:
return white(_that);case SwatchColorDto_Black() when black != null:
return black(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( int index)?  hue,TResult Function()?  white,TResult Function()?  black,required TResult orElse(),}) {final _that = this;
switch (_that) {
case SwatchColorDto_Hue() when hue != null:
return hue(_that.index);case SwatchColorDto_White() when white != null:
return white();case SwatchColorDto_Black() when black != null:
return black();case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( int index)  hue,required TResult Function()  white,required TResult Function()  black,}) {final _that = this;
switch (_that) {
case SwatchColorDto_Hue():
return hue(_that.index);case SwatchColorDto_White():
return white();case SwatchColorDto_Black():
return black();}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( int index)?  hue,TResult? Function()?  white,TResult? Function()?  black,}) {final _that = this;
switch (_that) {
case SwatchColorDto_Hue() when hue != null:
return hue(_that.index);case SwatchColorDto_White() when white != null:
return white();case SwatchColorDto_Black() when black != null:
return black();case _:
  return null;

}
}

}

/// @nodoc


class SwatchColorDto_Hue extends SwatchColorDto {
  const SwatchColorDto_Hue({required this.index}): super._();
  

/// Hue index.
 final  int index;

/// Create a copy of SwatchColorDto
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SwatchColorDto_HueCopyWith<SwatchColorDto_Hue> get copyWith => _$SwatchColorDto_HueCopyWithImpl<SwatchColorDto_Hue>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is SwatchColorDto_Hue&&(identical(other.index, index) || other.index == index));
}


@override
int get hashCode {
    return Object.hash(runtimeType,index);
}

@override
String toString() {
    return 'SwatchColorDto.hue(index: $index)';
}


}

/// @nodoc
abstract mixin class $SwatchColorDto_HueCopyWith<$Res> implements $SwatchColorDtoCopyWith<$Res> {
  factory $SwatchColorDto_HueCopyWith(SwatchColorDto_Hue value, $Res Function(SwatchColorDto_Hue) _then) = _$SwatchColorDto_HueCopyWithImpl;
@useResult
$Res call({
 int index
});




}
/// @nodoc
class _$SwatchColorDto_HueCopyWithImpl<$Res>
    implements $SwatchColorDto_HueCopyWith<$Res> {
  _$SwatchColorDto_HueCopyWithImpl(this._self, this._then);

  final SwatchColorDto_Hue _self;
  final $Res Function(SwatchColorDto_Hue) _then;

/// Create a copy of SwatchColorDto
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? index = null,}) {
  return _then(SwatchColorDto_Hue(
index: null == index ? _self.index : index // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class SwatchColorDto_White extends SwatchColorDto {
  const SwatchColorDto_White(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is SwatchColorDto_White);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'SwatchColorDto.white()';
}


}




/// @nodoc


class SwatchColorDto_Black extends SwatchColorDto {
  const SwatchColorDto_Black(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is SwatchColorDto_Black);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'SwatchColorDto.black()';
}


}




// dart format on
